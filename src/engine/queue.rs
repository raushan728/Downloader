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

use crate::config::manager::ConfigManager;
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

pub fn clear_global_queue() {
    if let Ok(mut g) = GLOBAL_QUEUE.lock() {
        *g = None;
    }
}

pub fn active_downloads_lock_path() -> Option<PathBuf> {
    ConfigManager::config_path().and_then(|p| p.parent().map(|d| d.join("active_downloads.lock")))
}

pub fn record_active_download() {
    if let Some(lock_path) = active_downloads_lock_path() {
        if let Some(parent) = lock_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let my_pid = std::process::id();
        let mut pids = Vec::new();
        if let Ok(content) = std::fs::read_to_string(&lock_path) {
            for line in content.lines() {
                if let Ok(pid) = line.trim().parse::<u32>()
                    && pid != 0
                    && !pids.contains(&pid)
                {
                    pids.push(pid);
                }
            }
        }
        if !pids.contains(&my_pid) {
            pids.push(my_pid);
        }
        let payload = pids
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let _ = std::fs::write(&lock_path, payload);
    }
}

pub fn clear_active_download() {
    if let Some(lock_path) = active_downloads_lock_path() {
        let my_pid = std::process::id();
        let mut pids = Vec::new();
        if let Ok(content) = std::fs::read_to_string(&lock_path) {
            for line in content.lines() {
                if let Ok(pid) = line.trim().parse::<u32>()
                    && pid != 0
                    && pid != my_pid
                    && !pids.contains(&pid)
                {
                    pids.push(pid);
                }
            }
        }
        if pids.is_empty() {
            let _ = std::fs::remove_file(&lock_path);
        } else {
            let payload = pids
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            let _ = std::fs::write(&lock_path, payload);
        }
    }
}

#[cfg(unix)]
pub fn is_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    std::path::Path::new(&format!("/proc/{}", pid)).exists()
        || Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
}

#[cfg(windows)]
pub fn is_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {}", pid), "/NH"])
        .output()
        .map(|o| {
            let out = String::from_utf8_lossy(&o.stdout);
            out.contains(&pid.to_string())
        })
        .unwrap_or(false)
}

pub fn is_download_active() -> bool {
    if let Ok(g) = GLOBAL_QUEUE.lock()
        && let Some(queue) = &*g
        && let Ok(guard) = queue.lock()
        && guard.is_active()
    {
        return true;
    }

    if crate::engine::downloader::is_direct_download_active() {
        return true;
    }

    if let Some(lock_path) = active_downloads_lock_path()
        && lock_path.exists()
        && let Ok(content) = std::fs::read_to_string(&lock_path)
    {
        let my_pid = std::process::id();
        let mut has_other_active = false;
        let mut surviving_pids = Vec::new();

        for line in content.lines() {
            if let Ok(pid) = line.trim().parse::<u32>() {
                if pid == 0 {
                    continue;
                }
                if pid == my_pid {
                    surviving_pids.push(pid);
                } else if is_pid_alive(pid) {
                    has_other_active = true;
                    surviving_pids.push(pid);
                }
            }
        }

        if surviving_pids.is_empty() {
            let _ = std::fs::remove_file(&lock_path);
        } else if surviving_pids.len() != content.lines().count() {
            let payload = surviving_pids
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            let _ = std::fs::write(&lock_path, payload);
        }

        if has_other_active {
            return true;
        }
    }

    false
}

pub fn cancel_global_downloads() {
    let g = match GLOBAL_QUEUE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(queue) = &*g {
        let mut guard = match queue.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.cancel_all();
    }
    clear_active_download();
}

/// Kills the entire process tree to prevent orphan child workers (e.g. ffmpeg) from surviving.
pub fn kill_process_by_pid(pid: u32) {
    if pid == 0 {
        return;
    }

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
        // Terminate the process group (-pid) to sweep both yt-dlp and child ffmpeg processes,
        // then fall back to killing the single PID directly.
        let _ = Command::new("kill")
            .arg("-9")
            .arg(format!("-{}", pid))
            .output();
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
        if mgr.cancelled.load(Ordering::SeqCst) && mgr.active_count == 0 && mgr.queue.is_empty() {
            mgr.cancelled = Arc::new(AtomicBool::new(false));
        }
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
            record_active_download();
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

            if mgr.active_count < mgr.max_parallel {
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
            record_active_download();
            Self::spawn_worker(manager_arc, next_task);
        } else {
            let is_now_idle = {
                let mgr = manager_arc.lock().unwrap();
                mgr.active_count == 0
            };
            if is_now_idle {
                clear_active_download();
            }
        }
    }

    /// Cancels queued and active tasks, terminates child process trees, and purges partial files.
    pub fn cancel_all(&mut self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.queue.clear();

        for task in &self.tasks {
            let mut t = match task.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
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
        clear_active_download();
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
                println!();
                return;
            }

            if !self.is_active() {
                println!("\n{}", "INFO: No active downloads in progress.".cyan());
                self.print_status();
                println!();
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

                let term_w = crate::engine::downloader::get_terminal_width();

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
                    let status_extra_len = if *status == TaskStatus::Finalizing {
                        24
                    } else {
                        0
                    };
                    let max_title_len = term_w.saturating_sub(15 + status_extra_len).max(10);
                    let title_display = if title.chars().count() > max_title_len {
                        let truncated: String = title
                            .chars()
                            .take(max_title_len.saturating_sub(3))
                            .collect();
                        format!("{}...", truncated)
                    } else {
                        title.clone()
                    };
                    println!("  [#{}] \"{}\"{}", id, title_display, status_str);
                    println!("       {}", bar_clean);
                    count += 2;
                }

                let _ = io::stdout().flush();
                lines_printed = count;
            }

            if lines_printed > 0 {
                let _ = crossterm::execute!(
                    io::stdout(),
                    crossterm::cursor::MoveUp(lines_printed),
                    crossterm::terminal::Clear(crossterm::terminal::ClearType::FromCursorDown)
                );
            }
            println!("{}", "INFO: Exited real-time status monitor.".cyan());
            self.print_status();
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

        mgr_arc.lock().unwrap().active_count = 1;
        assert!(is_download_active());

        mgr_arc.lock().unwrap().cancel_all();
        assert!(!is_download_active());

        clear_global_queue();
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

    #[test]
    fn test_queue_max_parallel_reduction() {
        let mgr_arc = QueueManager::new(3, PathBuf::from("yt-dlp"), PathBuf::from("ffmpeg"));
        {
            let mut mgr = mgr_arc.lock().unwrap();
            mgr.active_count = 3;
            mgr.max_parallel = 1; // Dynamically reduced to 1
            mgr.queue.push_back(100);
            mgr.tasks.push(Arc::new(Mutex::new(DownloadTask {
                id: 100,
                url: "url100".to_string(),
                title: "Queued Task 100".to_string(),
                format_id: "137".to_string(),
                download_dir: "/tmp".to_string(),
                status: TaskStatus::Queued,
                progress: TaskProgress::default(),
            })));
        }

        // Simulate one task finishing: active_count drops from 3 to 2.
        // Because active_count (2) is still >= max_parallel (1), task 100 must NOT be promoted!
        QueueManager::on_task_finished(&mgr_arc);

        {
            let mgr = mgr_arc.lock().unwrap();
            assert_eq!(mgr.active_count, 2);
            assert_eq!(mgr.queue.len(), 1);
            assert_eq!(mgr.queue[0], 100);
        }
    }

    #[test]
    fn test_queue_cancelled_latch_resets_on_enqueue() {
        let mgr_arc = QueueManager::new(2, PathBuf::from("yt-dlp"), PathBuf::from("ffmpeg"));
        {
            let mut mgr = mgr_arc.lock().unwrap();
            mgr.cancel_all();
            assert!(mgr.cancelled.load(Ordering::SeqCst));
            assert_eq!(mgr.active_count, 0);
            assert!(mgr.queue.is_empty());
        }

        let _ = QueueManager::enqueue(
            &mgr_arc,
            "url".to_string(),
            "Title".to_string(),
            "137".to_string(),
            "/tmp".to_string(),
        );

        let mgr = mgr_arc.lock().unwrap();
        assert!(!mgr.cancelled.load(Ordering::SeqCst));
    }
}
