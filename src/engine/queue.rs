use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use colored::Colorize;

use crate::engine::downloader::{download_media_background, render_progress_bar};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Queued,
    Downloading,
    Finalizing,
    Completed,
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct TaskProgress {
    pub percent: f64,
    pub downloaded_str: String,
    pub total_str: String,
    pub speed_str: String,
    pub eta_str: String,
}

impl Default for TaskProgress {
    fn default() -> Self {
        Self {
            percent: 0.0,
            downloaded_str: "0 B".to_string(),
            total_str: "Unknown".to_string(),
            speed_str: "Unknown".to_string(),
            eta_str: "--:--".to_string(),
        }
    }
}

pub struct DownloadTask {
    pub id: usize,
    pub url: String,
    pub title: String,
    pub format_id: String,
    pub download_dir: String,
    pub status: TaskStatus,
    pub progress: TaskProgress,
}

#[derive(Debug, Clone)]
pub enum EnqueueResult {
    Started {
        task_id: usize,
        active_count: usize,
        queued_count: usize,
    },
    Queued {
        task_id: usize,
        position: usize,
        active_count: usize,
        queued_count: usize,
    },
}

pub struct QueueManager {
    pub max_parallel: usize,
    next_task_id: usize,
    tasks: Vec<Arc<Mutex<DownloadTask>>>,
    queue: VecDeque<usize>,
    active_count: usize,
    active_pids: Vec<u32>,
    cancelled: Arc<AtomicBool>,
    yt_dlp_path: PathBuf,
    ffmpeg_path: PathBuf,
}

static GLOBAL_QUEUE: Mutex<Option<Arc<Mutex<QueueManager>>>> = Mutex::new(None);

pub fn set_global_queue(queue: Arc<Mutex<QueueManager>>) {
    if let Ok(mut g) = GLOBAL_QUEUE.lock() {
        *g = Some(queue);
    }
}

pub fn is_download_active() -> bool {
    if let Ok(g) = GLOBAL_QUEUE.lock()
        && let Some(queue) = &*g
        && let Ok(guard) = queue.lock()
    {
        return guard.is_active();
    }
    false
}

pub fn kill_process_by_pid(pid: u32) {
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("taskkill")
            .arg("/F")
            .arg("/T")
            .arg("/PID")
            .arg(pid.to_string())
            .output();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = Command::new("kill").arg("-9").arg(pid.to_string()).output();
    }
}

impl QueueManager {
    pub fn new(
        max_parallel: usize,
        yt_dlp_path: PathBuf,
        ffmpeg_path: PathBuf,
    ) -> Arc<Mutex<Self>> {
        let max_parallel = if max_parallel == 0 { 1 } else { max_parallel };
        Arc::new(Mutex::new(Self {
            max_parallel,
            next_task_id: 1,
            tasks: Vec::new(),
            queue: VecDeque::new(),
            active_count: 0,
            active_pids: Vec::new(),
            cancelled: Arc::new(AtomicBool::new(false)),
            yt_dlp_path,
            ffmpeg_path,
        }))
    }

    pub fn enqueue(
        manager_arc: &Arc<Mutex<Self>>,
        url: String,
        title: String,
        format_id: String,
        download_dir: String,
    ) -> EnqueueResult {
        let mut mgr = manager_arc.lock().unwrap();
        let task_id = mgr.next_task_id;
        mgr.next_task_id += 1;

        let task = Arc::new(Mutex::new(DownloadTask {
            id: task_id,
            url,
            title,
            format_id,
            download_dir,
            status: TaskStatus::Queued,
            progress: TaskProgress::default(),
        }));
        mgr.tasks.push(Arc::clone(&task));

        if mgr.active_count < mgr.max_parallel {
            mgr.active_count += 1;
            task.lock().unwrap().status = TaskStatus::Downloading;
            let result = EnqueueResult::Started {
                task_id,
                active_count: mgr.active_count,
                queued_count: mgr.queue.len(),
            };
            drop(mgr);

            Self::spawn_worker(manager_arc, task);
            result
        } else {
            mgr.queue.push_back(task_id);
            let position = mgr.queue.len();
            EnqueueResult::Queued {
                task_id,
                position,
                active_count: mgr.active_count,
                queued_count: mgr.queue.len(),
            }
        }
    }

    fn spawn_worker(manager_arc: &Arc<Mutex<Self>>, task: Arc<Mutex<DownloadTask>>) {
        let mgr_clone = Arc::clone(manager_arc);
        let task_clone = Arc::clone(&task);

        thread::spawn(move || {
            let (url, format_id, yt_dlp_path, ffmpeg_path, download_dir, cancelled) = {
                let mgr = mgr_clone.lock().unwrap();
                let t = task_clone.lock().unwrap();
                (
                    t.url.clone(),
                    t.format_id.clone(),
                    mgr.yt_dlp_path.clone(),
                    mgr.ffmpeg_path.clone(),
                    t.download_dir.clone(),
                    Arc::clone(&mgr.cancelled),
                )
            };

            let task_for_progress = Arc::clone(&task_clone);
            let mgr_for_spawn = Arc::clone(&mgr_clone);
            let mut spawned_pid: Option<u32> = None;

            let result = download_media_background(
                &url,
                &format_id,
                &yt_dlp_path,
                &ffmpeg_path,
                &download_dir,
                |child| {
                    let pid = child.id();
                    spawned_pid = Some(pid);
                    let mut mgr = mgr_for_spawn.lock().unwrap();
                    mgr.active_pids.push(pid);
                },
                move |percent, downloaded, total, speed, eta, is_finalizing| {
                    let mut t = task_for_progress.lock().unwrap();
                    if is_finalizing {
                        t.status = TaskStatus::Finalizing;
                        t.progress.percent = 100.0;
                    } else {
                        t.progress.percent = percent;
                        if !downloaded.is_empty() {
                            t.progress.downloaded_str = downloaded.to_string();
                        }
                        if !total.is_empty() {
                            t.progress.total_str = total.to_string();
                        }
                        if !speed.is_empty() {
                            t.progress.speed_str = speed.to_string();
                        }
                        if !eta.is_empty() {
                            t.progress.eta_str = eta.to_string();
                        }
                    }
                },
            );

            if let Some(pid) = spawned_pid {
                let mut mgr = mgr_clone.lock().unwrap();
                mgr.active_pids.retain(|&p| p != pid);
            }

            let is_cancelled = cancelled.load(Ordering::SeqCst);

            let (title, download_dir) = {
                let mut t = task_clone.lock().unwrap();
                if is_cancelled {
                    t.status = TaskStatus::Cancelled;
                } else {
                    match &result {
                        Ok(()) => {
                            t.status = TaskStatus::Completed;
                            t.progress.percent = 100.0;
                        }
                        Err(e) => {
                            t.status = TaskStatus::Failed(e.clone());
                        }
                    }
                }
                (t.title.clone(), t.download_dir.clone())
            };

            if !is_cancelled {
                match result {
                    Ok(()) => {
                        println!(
                            "\n{}",
                            format!(
                                "SUCCESS: Download completed: \"{}\" -> saved to {}",
                                title, download_dir
                            )
                            .green()
                            .bold()
                        );
                    }
                    Err(e) => {
                        println!(
                            "\n{}",
                            format!("ERROR: Download failed for \"{}\": {}", title, e)
                                .red()
                                .bold()
                        );
                    }
                }
            }

            Self::on_task_finished(&mgr_clone);
        });
    }

    fn on_task_finished(manager_arc: &Arc<Mutex<Self>>) {
        let next_task = {
            let mut mgr = manager_arc.lock().unwrap();
            if mgr.active_count > 0 {
                mgr.active_count -= 1;
            }

            if mgr.cancelled.load(Ordering::SeqCst) {
                return;
            }

            if let Some(next_id) = mgr.queue.pop_front() {
                let task_opt = mgr
                    .tasks
                    .iter()
                    .find(|t| t.lock().unwrap().id == next_id)
                    .cloned();
                if let Some(task) = task_opt {
                    mgr.active_count += 1;
                    task.lock().unwrap().status = TaskStatus::Downloading;
                    Some((task, mgr.active_count, mgr.queue.len(), mgr.max_parallel))
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some((next_task, active, queued, max_parallel)) = next_task {
            let title = next_task.lock().unwrap().title.clone();
            println!(
                "{}",
                format!(
                    "INFO: Starting queued download: \"{}\" [Active: {}/{}, Queued: {}]",
                    title, active, max_parallel, queued
                )
                .cyan()
            );
            Self::spawn_worker(manager_arc, next_task);
        }
    }

    pub fn cancel_all(&mut self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.queue.clear();

        for task in &self.tasks {
            let mut t = task.lock().unwrap();
            if matches!(
                t.status,
                TaskStatus::Queued | TaskStatus::Downloading | TaskStatus::Finalizing
            ) {
                t.status = TaskStatus::Cancelled;
            }
        }

        for &pid in &self.active_pids {
            kill_process_by_pid(pid);
        }
        self.active_pids.clear();
        self.active_count = 0;
    }

    pub fn is_active(&self) -> bool {
        self.active_count > 0 || !self.queue.is_empty()
    }

    pub fn print_status(&self) {
        let tasks_snapshot: Vec<(usize, String, TaskStatus, TaskProgress)> = self
            .tasks
            .iter()
            .map(|t| {
                let guard = t.lock().unwrap();
                (
                    guard.id,
                    guard.title.clone(),
                    guard.status.clone(),
                    guard.progress.clone(),
                )
            })
            .collect();

        let active_tasks: Vec<_> = tasks_snapshot
            .iter()
            .filter(|(_, _, s, _)| matches!(s, TaskStatus::Downloading | TaskStatus::Finalizing))
            .collect();

        let queued_tasks: Vec<_> = tasks_snapshot
            .iter()
            .filter(|(_, _, s, _)| matches!(s, TaskStatus::Queued))
            .collect();

        println!(
            "\n{}",
            format!(
                "=== Active Downloads ({}/{}) ===",
                active_tasks.len(),
                self.max_parallel
            )
            .bold()
        );

        if active_tasks.is_empty() {
            println!("  (None)");
        } else {
            for (id, title, status, prog) in &active_tasks {
                let bar = render_progress_bar(
                    prog.percent,
                    &prog.downloaded_str,
                    &prog.total_str,
                    &prog.speed_str,
                    &prog.eta_str,
                );
                let bar_clean = bar.trim_start_matches('\r').trim_end();
                let status_str = if *status == TaskStatus::Finalizing {
                    " [Finalizing media...]".yellow()
                } else {
                    "".normal()
                };
                println!("  [#{}] \"{}\"{}", id, title, status_str);
                println!("       {}", bar_clean);
            }
        }

        println!(
            "\n{}",
            format!("=== Queued Downloads ({}) ===", queued_tasks.len()).bold()
        );

        if queued_tasks.is_empty() {
            println!("  (None)");
        } else {
            for (i, (id, title, _, _)) in queued_tasks.iter().enumerate() {
                println!("  [#{}] (Position: {}) \"{}\"", id, i + 1, title);
            }
        }
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_enqueue_and_capacity() {
        let mgr_arc = QueueManager::new(2, PathBuf::from("yt-dlp"), PathBuf::from("ffmpeg"));
        assert_eq!(mgr_arc.lock().unwrap().max_parallel, 2);
        assert!(!mgr_arc.lock().unwrap().is_active());

        // First task -> should start
        let res1 = QueueManager::enqueue(
            &mgr_arc,
            "url1".to_string(),
            "Title 1".to_string(),
            "248".to_string(),
            "/tmp".to_string(),
        );
        match res1 {
            EnqueueResult::Started {
                task_id,
                active_count,
                queued_count,
            } => {
                assert_eq!(task_id, 1);
                assert_eq!(active_count, 1);
                assert_eq!(queued_count, 0);
            }
            _ => panic!("Expected task 1 to start"),
        }

        // Second task -> should start
        let res2 = QueueManager::enqueue(
            &mgr_arc,
            "url2".to_string(),
            "Title 2".to_string(),
            "248".to_string(),
            "/tmp".to_string(),
        );
        match res2 {
            EnqueueResult::Started {
                task_id,
                active_count,
                queued_count,
            } => {
                assert_eq!(task_id, 2);
                assert_eq!(active_count, 2);
                assert_eq!(queued_count, 0);
            }
            _ => panic!("Expected task 2 to start"),
        }

        // Third task -> should be queued
        let res3 = QueueManager::enqueue(
            &mgr_arc,
            "url3".to_string(),
            "Title 3".to_string(),
            "248".to_string(),
            "/tmp".to_string(),
        );
        match res3 {
            EnqueueResult::Queued {
                task_id,
                position,
                active_count,
                queued_count,
            } => {
                assert_eq!(task_id, 3);
                assert_eq!(position, 1);
                assert_eq!(active_count, 2);
                assert_eq!(queued_count, 1);
            }
            _ => panic!("Expected task 3 to be queued"),
        }

        assert!(mgr_arc.lock().unwrap().is_active());
    }

    #[test]
    fn test_queue_cancel_all() {
        let mgr_arc = QueueManager::new(1, PathBuf::from("yt-dlp"), PathBuf::from("ffmpeg"));
        let _ = QueueManager::enqueue(
            &mgr_arc,
            "url1".to_string(),
            "Title 1".to_string(),
            "248".to_string(),
            "/tmp".to_string(),
        );
        let _ = QueueManager::enqueue(
            &mgr_arc,
            "url2".to_string(),
            "Title 2".to_string(),
            "248".to_string(),
            "/tmp".to_string(),
        );

        mgr_arc.lock().unwrap().cancel_all();

        let mgr = mgr_arc.lock().unwrap();
        assert_eq!(mgr.active_count, 0);
        assert!(mgr.queue.is_empty());
        assert!(!mgr.is_active());
    }

    #[test]
    fn test_global_queue_is_download_active() {
        let mgr_arc = QueueManager::new(2, PathBuf::from("yt-dlp"), PathBuf::from("ffmpeg"));
        set_global_queue(Arc::clone(&mgr_arc));
        assert!(!is_download_active());

        let _ = QueueManager::enqueue(
            &mgr_arc,
            "url1".to_string(),
            "Title 1".to_string(),
            "248".to_string(),
            "/tmp".to_string(),
        );
        assert!(is_download_active());

        mgr_arc.lock().unwrap().cancel_all();
        assert!(!is_download_active());
    }
}
