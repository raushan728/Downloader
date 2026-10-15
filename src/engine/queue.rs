use std::collections::VecDeque;
#[cfg(not(test))]
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
#[cfg(not(test))]
use std::time::Duration;

use colored::Colorize;

use crate::engine::downloader::{
    clean_partial_files, download_media_background, render_progress_bar,
};

/// Lifecycle states of a queued or active media download task.
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

/// Download unit containing media metadata, destination, and live state.
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

/// FIFO worker pool bounding concurrent media downloads to `max_parallel`.
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

pub fn cancel_global_downloads() {
    if let Ok(g) = GLOBAL_QUEUE.lock()
        && let Some(queue) = &*g
        && let Ok(mut guard) = queue.lock()
    {
        guard.cancel_all();
    }
}

/// Kills the entire process tree to prevent orphan child workers (e.g. ffmpeg) from surviving.
pub fn kill_process_by_pid(pid: u32) {
    #[cfg(target_os = "windows")]
    {
        // /T flag recurses through spawned child processes, /F forces immediate termination.
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
        // Prevent 0 concurrency causing tasks to queue indefinitely without processing.
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

            if is_cancelled || result.is_err() {
                clean_partial_files(&download_dir, Some(&title));
            }

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

    // Automatically promotes and spawns the next pending task in FIFO order.
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

    /// Cancels queued and active tasks, terminates child process trees, and purges partial files.
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
                clean_partial_files(&t.download_dir, Some(&t.title));
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

    pub fn print_status_live(&self) {
        #[cfg(test)]
        {
            self.print_status();
        }
        #[cfg(not(test))]
        {
            if !io::stdout().is_terminal() || !io::stdin().is_terminal() {
                self.print_status();
                return;
            }

            if !self.is_active() {
                println!("\n{}", "INFO: No active downloads in progress.".cyan());
                self.print_status();
                return;
            }

            println!(
                "\n{}",
                "=== Real-Time Download Status (Press Enter to exit live view) ===".bold()
            );

            let mut lines_printed: u16 = 0;

            loop {
                if let Ok(true) = crossterm::event::poll(Duration::from_millis(200))
                    && let Ok(crossterm::event::Event::Key(_)) = crossterm::event::read()
                {
                    break;
                }

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
                    .filter(|(_, _, s, _)| {
                        matches!(s, TaskStatus::Downloading | TaskStatus::Finalizing)
                    })
                    .collect();

                let queued_count = tasks_snapshot
                    .iter()
                    .filter(|(_, _, s, _)| matches!(s, TaskStatus::Queued))
                    .count();

                if lines_printed > 0 {
                    let _ = crossterm::execute!(
                        io::stdout(),
                        crossterm::cursor::MoveUp(lines_printed),
                        crossterm::terminal::Clear(crossterm::terminal::ClearType::FromCursorDown)
                    );
                }

                if active_tasks.is_empty() && queued_count == 0 {
                    println!("{}", "SUCCESS: All active tasks completed!".green());
                    break;
                }

                let mut count = 0;
                println!(
                    "Active Downloads: {}/{} | Queued in line: {}",
                    active_tasks.len(),
                    self.max_parallel,
                    queued_count
                );
                count += 1;

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
                    count += 2;
                }

                let _ = io::stdout().flush();
                lines_printed = count;
            }

            println!();
        }
    }

    pub fn print_list(&self) {
        let tasks_snapshot: Vec<(usize, String, String, TaskStatus, TaskProgress)> = self
            .tasks
            .iter()
            .map(|t| {
                let guard = t.lock().unwrap();
                (
                    guard.id,
                    guard.title.clone(),
                    guard.download_dir.clone(),
                    guard.status.clone(),
                    guard.progress.clone(),
                )
            })
            .collect();

        if tasks_snapshot.is_empty() {
            println!(
                "\n{}\n",
                "INFO: No downloads recorded in this session.".cyan()
            );
            return;
        }

        println!("\n{}", "=== Download History & Session Report ===".bold());
        for (id, title, dir, status, prog) in &tasks_snapshot {
            match status {
                TaskStatus::Downloading => {
                    println!(
                        "  [#{}] \"{}\" - {} ({:.1}%)",
                        id,
                        title,
                        "DOWNLOADING".cyan().bold(),
                        prog.percent
                    );
                    println!(
                        "       {} / {} at {} (ETA: {})",
                        prog.downloaded_str, prog.total_str, prog.speed_str, prog.eta_str
                    );
                }
                TaskStatus::Finalizing => {
                    println!(
                        "  [#{}] \"{}\" - {}",
                        id,
                        title,
                        "FINALIZING MEDIA".yellow().bold()
                    );
                }
                TaskStatus::Queued => {
                    println!("  [#{}] \"{}\" - {}", id, title, "QUEUED".blue());
                }
                TaskStatus::Completed => {
                    println!(
                        "  [#{}] \"{}\" - {} -> {}",
                        id,
                        title,
                        "COMPLETED".green().bold(),
                        dir
                    );
                }
                TaskStatus::Failed(err) => {
                    println!(
                        "  [#{}] \"{}\" - {} ({})",
                        id,
                        title,
                        "FAILED".red().bold(),
                        err
                    );
                }
                TaskStatus::Cancelled => {
                    println!("  [#{}] \"{}\" - {}", id, title, "CANCELLED".yellow());
                }
            }
        }
        println!();
    }

    pub fn print_queue(&self) {
        let tasks_snapshot: Vec<(usize, String, TaskStatus)> = self
            .tasks
            .iter()
            .map(|t| {
                let guard = t.lock().unwrap();
                (guard.id, guard.title.clone(), guard.status.clone())
            })
            .collect();

        let queued_tasks: Vec<_> = tasks_snapshot
            .iter()
            .filter(|(_, _, s)| matches!(s, TaskStatus::Queued))
            .collect();

        let active_count = tasks_snapshot
            .iter()
            .filter(|(_, _, s)| matches!(s, TaskStatus::Downloading | TaskStatus::Finalizing))
            .count();

        println!(
            "\n{}",
            format!(
                "=== Download Queue (Active Slots: {}/{}, Queued in Line: {}) ===",
                active_count,
                self.max_parallel,
                queued_tasks.len()
            )
            .bold()
        );

        if queued_tasks.is_empty() {
            println!("  (Queue is empty)");
        } else {
            for (i, (id, title, _)) in queued_tasks.iter().enumerate() {
                println!("  [#{}] Position #{}: \"{}\"", id, i + 1, title);
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
    #[test]
    fn test_queue_zero_parallel() {
        let mgr_arc = QueueManager::new(0, PathBuf::from("yt-dlp"), PathBuf::from("ffmpeg"));
        assert_eq!(mgr_arc.lock().unwrap().max_parallel, 1);
    }

    #[test]
    fn test_task_progress_default() {
        let prog = TaskProgress::default();
        assert_eq!(prog.percent, 0.0);
        assert_eq!(prog.downloaded_str, "0 B");
        assert_eq!(prog.total_str, "Unknown");
        assert_eq!(prog.speed_str, "Unknown");
        assert_eq!(prog.eta_str, "--:--");
    }

    #[test]
    fn test_queue_cancel_when_empty() {
        let mgr_arc = QueueManager::new(3, PathBuf::from("yt-dlp"), PathBuf::from("ffmpeg"));
        mgr_arc.lock().unwrap().cancel_all();
        let mgr = mgr_arc.lock().unwrap();
        assert_eq!(mgr.active_count, 0);
        assert!(mgr.queue.is_empty());
        assert!(!mgr.is_active());
    }

    #[test]
    fn test_queue_multiple_enqueues() {
        let mgr_arc = QueueManager::new(2, PathBuf::from("yt-dlp"), PathBuf::from("ffmpeg"));
        for i in 1..=5 {
            let res = QueueManager::enqueue(
                &mgr_arc,
                format!("url{}", i),
                format!("Title {}", i),
                "248".to_string(),
                "/tmp".to_string(),
            );
            if i <= 2 {
                match res {
                    EnqueueResult::Started { active_count, .. } => {
                        assert_eq!(active_count, i);
                    }
                    _ => panic!("Task {} should have started", i),
                }
            } else {
                match res {
                    EnqueueResult::Queued { position, .. } => {
                        assert_eq!(position, i - 2);
                    }
                    _ => panic!("Task {} should have queued", i),
                }
            }
        }

        let mgr = mgr_arc.lock().unwrap();
        assert_eq!(mgr.active_count, 2);
        assert_eq!(mgr.queue.len(), 3);
        assert!(mgr.is_active());
        drop(mgr);

        // Printing status should not panic
        mgr_arc.lock().unwrap().print_status();

        mgr_arc.lock().unwrap().cancel_all();
        assert!(!mgr_arc.lock().unwrap().is_active());
    }

    #[test]
    fn test_queue_print_list_with_various_task_statuses() {
        let mgr_arc = QueueManager::new(2, PathBuf::from("yt-dlp"), PathBuf::from("ffmpeg"));
        let mut mgr = mgr_arc.lock().unwrap();

        mgr.print_list();

        mgr.tasks.push(Arc::new(Mutex::new(DownloadTask {
            id: 1,
            url: "https://example.com/1".to_string(),
            title: "Downloading Video".to_string(),
            format_id: "137".to_string(),
            download_dir: "/downloads".to_string(),
            status: TaskStatus::Downloading,
            progress: TaskProgress {
                percent: 45.5,
                downloaded_str: "45 MiB".to_string(),
                total_str: "100 MiB".to_string(),
                speed_str: "2.5 MiB/s".to_string(),
                eta_str: "00:22".to_string(),
            },
        })));

        mgr.tasks.push(Arc::new(Mutex::new(DownloadTask {
            id: 2,
            url: "https://example.com/2".to_string(),
            title: "Finalizing Video".to_string(),
            format_id: "137".to_string(),
            download_dir: "/downloads".to_string(),
            status: TaskStatus::Finalizing,
            progress: TaskProgress::default(),
        })));

        mgr.tasks.push(Arc::new(Mutex::new(DownloadTask {
            id: 3,
            url: "https://example.com/3".to_string(),
            title: "Queued Video".to_string(),
            format_id: "137".to_string(),
            download_dir: "/downloads".to_string(),
            status: TaskStatus::Queued,
            progress: TaskProgress::default(),
        })));

        mgr.tasks.push(Arc::new(Mutex::new(DownloadTask {
            id: 4,
            url: "https://example.com/4".to_string(),
            title: "Completed Video".to_string(),
            format_id: "137".to_string(),
            download_dir: "/downloads".to_string(),
            status: TaskStatus::Completed,
            progress: TaskProgress {
                percent: 100.0,
                downloaded_str: "100 MiB".to_string(),
                total_str: "100 MiB".to_string(),
                speed_str: "0 B/s".to_string(),
                eta_str: "00:00".to_string(),
            },
        })));

        mgr.tasks.push(Arc::new(Mutex::new(DownloadTask {
            id: 5,
            url: "https://example.com/5".to_string(),
            title: "Failed Video".to_string(),
            format_id: "137".to_string(),
            download_dir: "/downloads".to_string(),
            status: TaskStatus::Failed("Disk space exhausted".to_string()),
            progress: TaskProgress::default(),
        })));

        mgr.tasks.push(Arc::new(Mutex::new(DownloadTask {
            id: 6,
            url: "https://example.com/6".to_string(),
            title: "Cancelled Video".to_string(),
            format_id: "137".to_string(),
            download_dir: "/downloads".to_string(),
            status: TaskStatus::Cancelled,
            progress: TaskProgress::default(),
        })));

        mgr.print_list();
    }

    #[test]
    fn test_queue_print_queue_and_status_live_empty_and_populated() {
        let mgr_arc = QueueManager::new(2, PathBuf::from("yt-dlp"), PathBuf::from("ffmpeg"));
        let mut mgr = mgr_arc.lock().unwrap();

        mgr.print_queue();
        mgr.print_status_live();

        mgr.queue.push_back(10);
        mgr.queue.push_back(11);
        mgr.tasks.push(Arc::new(Mutex::new(DownloadTask {
            id: 10,
            url: "url10".to_string(),
            title: "Queued Task 10".to_string(),
            format_id: "137".to_string(),
            download_dir: "/tmp".to_string(),
            status: TaskStatus::Queued,
            progress: TaskProgress::default(),
        })));
        mgr.tasks.push(Arc::new(Mutex::new(DownloadTask {
            id: 11,
            url: "url11".to_string(),
            title: "Queued Task 11".to_string(),
            format_id: "137".to_string(),
            download_dir: "/tmp".to_string(),
            status: TaskStatus::Queued,
            progress: TaskProgress::default(),
        })));

        assert!(mgr.is_active());
        mgr.print_queue();
        mgr.print_status_live();
    }
}
