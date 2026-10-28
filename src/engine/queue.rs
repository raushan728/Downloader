use std::collections::VecDeque;
#[cfg(not(test))]
use std::io::{self, IsTerminal};
use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use colored::Colorize;
use crossterm::event::KeyCode;
use fs2::FileExt;

use crate::config::manager::ConfigManager;
use crate::engine::downloader::{
    download_media_background_full, get_terminal_width, is_auth_or_bot_error,
    is_direct_download_active, is_rate_limit_error, render_progress_bar_with_width,
    sanitize_media_title,
};
use crate::engine::updater::check_core_engines_update;
#[cfg(not(test))]
use crate::ui::UiContext;
use crate::ui::progress::{
    DownloadPhase, ProgressState, format_bytes, format_eta, format_speed, render_foreground_frame,
};
use crate::ui::smooth::SpeedSmoother;
#[cfg(not(test))]
use crate::ui::terminal_title::set_terminal_title;
use crate::ui::text::{sanitize_display, truncate_display, visible_width};
#[cfg(not(test))]
use crate::ui::theme::TICK_MS;
use crate::ui::theme::{ColorLevel, SAMPLE_INTERVAL_MS, ThemeMode};

static IN_INTERACTIVE_PROMPT: AtomicBool = AtomicBool::new(false);
static NOTIFICATION_QUEUE: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn set_in_interactive_prompt(active: bool) {
    IN_INTERACTIVE_PROMPT.store(active, Ordering::SeqCst);
}

pub fn push_notification(msg: String) {
    if IN_INTERACTIVE_PROMPT.load(Ordering::SeqCst)
        && let Ok(mut q) = NOTIFICATION_QUEUE.lock()
    {
        q.push(msg);
        return;
    }
    println!("{}", msg);
}

pub fn flush_notifications() {
    let messages = {
        if let Ok(mut q) = NOTIFICATION_QUEUE.lock() {
            std::mem::take(&mut *q)
        } else {
            Vec::new()
        }
    };
    for msg in messages {
        println!("{}", msg);
    }
}

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
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed_bytes_sec: f64,
    pub eta_seconds: Option<u64>,
    pub phase: DownloadPhase,
    pub sparkline_samples: Vec<f64>,
    pub downloaded_str: String,
    pub total_str: String,
    pub speed_str: String,
    pub eta_str: String,
}

impl Default for TaskProgress {
    fn default() -> Self {
        Self {
            percent: 0.0,
            downloaded_bytes: 0,
            total_bytes: 0,
            speed_bytes_sec: 0.0,
            eta_seconds: None,
            phase: DownloadPhase::Video,
            sparkline_samples: Vec::new(),
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
    pub format_desc: String,
    pub download_dir: String,
    pub status: TaskStatus,
    pub progress: TaskProgress,
    pub custom_output_template: Option<String>,
    pub force_overwrite: bool,
    pub cookies: Option<String>,
    pub final_filepath: Option<PathBuf>,
    pub smoother: SpeedSmoother,
    pub last_sample_time: Option<Instant>,
}

#[derive(Debug, Clone)]
pub struct TaskSnapshot {
    pub id: usize,
    pub title: String,
    pub format_desc: String,
    pub status: TaskStatus,
    pub progress: TaskProgress,
}

#[derive(Debug, Clone)]
pub struct StatusSnapshot {
    pub active_tasks: Vec<TaskSnapshot>,
    pub max_parallel: usize,
    pub queued_count: usize,
    pub total_speed: f64,
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
    /// Disables background worker thread execution for offline unit testing.
    #[doc(hidden)]
    pub auto_spawn: bool,
    next_task_id: usize,
    tasks: Vec<Arc<Mutex<DownloadTask>>>,
    queue: VecDeque<usize>,
    active_count: usize,
    active_pids: Vec<u32>,
    worker_handles: Vec<thread::JoinHandle<()>>,
    cancelled: Arc<AtomicBool>,
    yt_dlp_path: PathBuf,
    ffmpeg_path: PathBuf,
    pub cookies: Option<String>,
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

/// Resolves the filesystem path to the advisory download lockfile (`~/.dwdr/active_downloads.lock`).
pub fn active_downloads_lock_path() -> Option<PathBuf> {
    ConfigManager::config_path().and_then(|p| p.parent().map(|d| d.join("active_downloads.lock")))
}

static ACTIVE_DOWNLOAD_LOCK_FILE: Mutex<Option<std::fs::File>> = Mutex::new(None);

/// Acquires an exclusive advisory lock on `active_downloads.lock` and records the current process PID.
///
/// Retains the open file descriptor in `ACTIVE_DOWNLOAD_LOCK_FILE` for the session duration
/// so other DWDR processes or config mutations can detect active downloads.
pub fn record_active_download() {
    let mut guard = match ACTIVE_DOWNLOAD_LOCK_FILE.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    if guard.is_none()
        && let Some(lock_path) = active_downloads_lock_path()
    {
        if let Some(parent) = lock_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            && file.try_lock_exclusive().is_ok()
        {
            let _ = file.set_len(0);
            let _ = file.seek(SeekFrom::Start(0));
            let my_pid = std::process::id();
            let _ = file.write_all(my_pid.to_string().as_bytes());
            let _ = file.flush();
            *guard = Some(file);
        }
    }
}

/// Releases the advisory file lock and deletes `active_downloads.lock` on disk.
pub fn clear_active_download() {
    let mut guard = match ACTIVE_DOWNLOAD_LOCK_FILE.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    if let Some(file) = guard.take() {
        let _ = file.unlock();
    }
    if let Some(lock_path) = active_downloads_lock_path() {
        let _ = std::fs::remove_file(&lock_path);
    }
}

#[cfg(unix)]
/// Returns true if a process with the given PID is running.
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
/// Returns true if a process with the given PID is running.
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

/// Returns true if downloads are active in this process or another running instance.
///
/// If `active_downloads.lock` exists on disk but can be exclusively locked by this process,
/// the previous holder exited or crashed; the recorded PID is verified via `is_pid_alive`
/// so stale lock files from abnormal terminations do not block subsequent operations.
pub fn is_download_active() -> bool {
    if let Ok(g) = GLOBAL_QUEUE.lock()
        && let Some(queue) = &*g
        && let Ok(guard) = queue.lock()
        && guard.is_active()
    {
        return true;
    }

    if is_direct_download_active() {
        return true;
    }

    if let Some(lock_path) = active_downloads_lock_path()
        && lock_path.exists()
        && let Ok(file) = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock_path)
    {
        match file.try_lock_exclusive() {
            Ok(()) => {
                let _ = file.unlock();
                if let Ok(content) = std::fs::read_to_string(&lock_path) {
                    let my_pid = std::process::id();
                    for line in content.lines() {
                        if let Ok(pid) = line.trim().parse::<u32>()
                            && pid != 0
                            && (pid == my_pid || is_pid_alive(pid))
                        {
                            return true;
                        }
                    }
                }
                return false;
            }
            Err(_) => {
                return true;
            }
        }
    }

    false
}

pub fn cancel_global_downloads() {
    let queue_opt = {
        let g = match GLOBAL_QUEUE.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        g.as_ref().map(Arc::clone)
    };
    if let Some(queue) = queue_opt {
        QueueManager::cancel_and_join(&queue);
    }
    clear_active_download();
}

/// Terminates an entire process tree to prevent orphan child workers (e.g. ffmpeg) from surviving.
///
/// On Unix, broadcasts `SIGTERM` to the process group (`libc::killpg`), polls for clean exit up to
/// 500ms, and escalates to uncatchable `SIGKILL` if the process remains active.
/// On Windows, issues a graceful `taskkill /T /PID` tree sweep, polls for 500ms, and escalates to `/F`.
pub fn kill_process_by_pid(pid: u32) {
    if pid == 0 || pid > i32::MAX as u32 {
        return;
    }

    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("taskkill")
            .arg("/T")
            .arg("/PID")
            .arg(pid.to_string())
            .output();

        let start = std::time::Instant::now();
        let mut still_alive = true;
        while start.elapsed() < std::time::Duration::from_millis(500) {
            if !is_pid_alive(pid) {
                still_alive = false;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        if still_alive {
            let _ = Command::new("taskkill")
                .arg("/F")
                .arg("/T")
                .arg("/PID")
                .arg(pid.to_string())
                .output();
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        // SAFETY: pid is validated to be non-zero and within pid_t bounds; SIGTERM is a valid POSIX signal.
        unsafe {
            libc::killpg(pid as libc::pid_t, libc::SIGTERM);
            libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }

        let start = std::time::Instant::now();
        let mut still_alive = true;
        while start.elapsed() < std::time::Duration::from_millis(500) {
            // SAFETY: pid is validated to be non-zero; signal 0 performs existence check without killing.
            if unsafe { libc::kill(pid as libc::pid_t, 0) } != 0 {
                still_alive = false;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        if still_alive {
            // SAFETY: pid is validated to be non-zero; SIGKILL is a valid POSIX signal.
            unsafe {
                libc::killpg(pid as libc::pid_t, libc::SIGKILL);
                libc::kill(pid as libc::pid_t, libc::SIGKILL);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveAction {
    Detach,
    AskCancel,
    ConfirmCancel,
    DismissCancel,
    None,
}

/// Maps keyboard events to interactive actions in the real-time live monitor.
///
/// In normal view (`confirming == false`), `'c'` initiates cancellation, while `'q'`, `'d'`,
/// `Esc`, and `Enter` detach cleanly back to the REPL shell without cancelling downloads.
/// When confirming (`confirming == true`), `'y'` confirms cancellation, while any other key dismisses.
pub fn handle_live_key(code: KeyCode, confirming: bool) -> LiveAction {
    if confirming {
        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => LiveAction::ConfirmCancel,
            _ => LiveAction::DismissCancel,
        }
    } else {
        match code {
            KeyCode::Char('c') | KeyCode::Char('C') => LiveAction::AskCancel,
            KeyCode::Char('d')
            | KeyCode::Char('D')
            | KeyCode::Char('q')
            | KeyCode::Char('Q')
            | KeyCode::Esc
            | KeyCode::Enter => LiveAction::Detach,
            _ => LiveAction::None,
        }
    }
}

#[cfg(not(test))]
struct RawModeGuard {
    active: bool,
}

#[cfg(not(test))]
impl RawModeGuard {
    fn new() -> Self {
        let active = crossterm::terminal::enable_raw_mode().is_ok();
        if active {
            let _ = crossterm::execute!(io::stdout(), crossterm::cursor::Hide);
        }
        Self { active }
    }

    fn restore(&mut self) {
        if self.active {
            let _ = crossterm::terminal::disable_raw_mode();
            let _ = crossterm::execute!(io::stdout(), crossterm::cursor::Show);
            self.active = false;
        }
    }
}

#[cfg(not(test))]
impl Drop for RawModeGuard {
    fn drop(&mut self) {
        self.restore();
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
            auto_spawn: true,
            next_task_id: 1,
            tasks: Vec::new(),
            queue: VecDeque::new(),
            active_count: 0,
            active_pids: Vec::new(),
            worker_handles: Vec::new(),
            cancelled: Arc::new(AtomicBool::new(false)),
            yt_dlp_path,
            ffmpeg_path,
            cookies: None,
        }))
    }

    pub fn set_cookies(&mut self, cookies: Option<String>) {
        self.cookies = cookies;
    }

    #[allow(clippy::too_many_arguments)]
    fn update_task_progress(
        task: &Arc<Mutex<DownloadTask>>,
        percent: f64,
        downloaded_bytes: u64,
        total_bytes: u64,
        speed_bytes_sec: f64,
        eta_seconds: Option<u64>,
        phase: DownloadPhase,
        is_finalizing: bool,
    ) {
        let mut t = task.lock().unwrap_or_else(|p| p.into_inner());
        if is_finalizing {
            t.status = TaskStatus::Finalizing;
            t.progress.percent = 100.0;
            t.progress.phase = phase;
            t.progress.speed_bytes_sec = 0.0;
            t.progress.speed_str = format_speed(0.0);
            t.progress.eta_seconds = None;
            t.progress.eta_str = "00:00".to_string();
        } else {
            t.progress.percent = percent;
            t.progress.downloaded_bytes = downloaded_bytes;
            t.progress.total_bytes = total_bytes;
            t.progress.speed_bytes_sec = speed_bytes_sec;
            t.progress.phase = phase;
            let now = Instant::now();
            let smoothed_speed = if t.last_sample_time.is_none_or(|prev| {
                now.duration_since(prev) >= Duration::from_millis(SAMPLE_INTERVAL_MS)
            }) {
                t.last_sample_time = Some(now);
                t.smoother.feed(speed_bytes_sec, now)
            } else {
                t.smoother.ema
            };
            t.progress.sparkline_samples = t.smoother.samples.iter().copied().collect();
            let rem = total_bytes.saturating_sub(downloaded_bytes);
            let eff_eta = t.smoother.eta_seconds(rem).or(eta_seconds);
            t.progress.eta_seconds = eff_eta;
            t.progress.downloaded_str = format_bytes(downloaded_bytes);
            t.progress.total_str = if total_bytes > 0 {
                format_bytes(total_bytes)
            } else {
                "Unknown".to_string()
            };
            t.progress.speed_str = format_speed(smoothed_speed);
            t.progress.eta_str = format_eta(eff_eta);
        }
    }

    pub fn enqueue(
        manager_arc: &Arc<Mutex<Self>>,
        url: String,
        title: String,
        format_id: String,
        download_dir: String,
    ) -> EnqueueResult {
        Self::enqueue_ext(
            manager_arc,
            url,
            title,
            format_id,
            download_dir,
            None,
            false,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn enqueue_ext(
        manager_arc: &Arc<Mutex<Self>>,
        url: String,
        title: String,
        format_id: String,
        download_dir: String,
        custom_output_template: Option<String>,
        force_overwrite: bool,
        format_desc: Option<String>,
        cookies: Option<String>,
    ) -> EnqueueResult {
        let mut mgr = manager_arc.lock().unwrap_or_else(|p| p.into_inner());
        if mgr.cancelled.load(Ordering::SeqCst) && mgr.active_count == 0 && mgr.queue.is_empty() {
            mgr.cancelled = Arc::new(AtomicBool::new(false));
        }
        let task_id = mgr.next_task_id;
        mgr.next_task_id += 1;

        let desc = format_desc.unwrap_or_else(|| format_id.clone());
        let task = Arc::new(Mutex::new(DownloadTask {
            id: task_id,
            url,
            title,
            format_id,
            format_desc: desc,
            download_dir,
            status: TaskStatus::Queued,
            progress: TaskProgress::default(),
            custom_output_template,
            force_overwrite,
            cookies,
            final_filepath: None,
            smoother: SpeedSmoother::default(),
            last_sample_time: None,
        }));
        mgr.tasks.push(Arc::clone(&task));

        if mgr.active_count < mgr.max_parallel {
            mgr.active_count += 1;
            task.lock().unwrap_or_else(|p| p.into_inner()).status = TaskStatus::Downloading;
            let result = EnqueueResult::Started {
                task_id,
                active_count: mgr.active_count,
                queued_count: mgr.queue.len(),
            };
            let auto_spawn = mgr.auto_spawn;
            drop(mgr);

            if auto_spawn {
                record_active_download();
                Self::spawn_worker(manager_arc, task);
            }
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

    pub fn set_max_parallel(manager_arc: &Arc<Mutex<Self>>, new_limit: usize) {
        let new_limit = if new_limit == 0 { 1 } else { new_limit };
        let (to_spawn, auto_spawn): (Vec<Arc<Mutex<DownloadTask>>>, bool) = {
            let mut mgr = manager_arc.lock().unwrap_or_else(|p| p.into_inner());
            mgr.max_parallel = new_limit;
            let mut tasks = Vec::new();

            if !mgr.cancelled.load(Ordering::SeqCst) {
                while mgr.active_count < mgr.max_parallel && !mgr.queue.is_empty() {
                    if let Some(next_id) = mgr.queue.pop_front() {
                        let task_opt = mgr
                            .tasks
                            .iter()
                            .find(|t| t.lock().unwrap_or_else(|p| p.into_inner()).id == next_id)
                            .cloned();
                        if let Some(task) = task_opt {
                            mgr.active_count += 1;
                            task.lock().unwrap_or_else(|p| p.into_inner()).status =
                                TaskStatus::Downloading;
                            tasks.push(task);
                        }
                    }
                }
            }
            (tasks, mgr.auto_spawn)
        };

        if !to_spawn.is_empty() && auto_spawn {
            record_active_download();
            for task in to_spawn {
                Self::spawn_worker(manager_arc, task);
            }
        }
    }

    fn spawn_worker(manager_arc: &Arc<Mutex<Self>>, task: Arc<Mutex<DownloadTask>>) {
        let mgr_clone = Arc::clone(manager_arc);
        let task_clone = Arc::clone(&task);

        let handle = thread::spawn(move || {
            let (
                task_id,
                url,
                format_id,
                yt_dlp_path,
                ffmpeg_path,
                download_dir,
                custom_output_template,
                force_overwrite,
                cancelled,
                task_cookies,
                mgr_cookies,
            ) = {
                let mgr = mgr_clone.lock().unwrap_or_else(|p| p.into_inner());
                let t = task_clone.lock().unwrap_or_else(|p| p.into_inner());
                (
                    t.id,
                    t.url.clone(),
                    t.format_id.clone(),
                    mgr.yt_dlp_path.clone(),
                    mgr.ffmpeg_path.clone(),
                    t.download_dir.clone(),
                    t.custom_output_template.clone(),
                    t.force_overwrite,
                    Arc::clone(&mgr.cancelled),
                    t.cookies.clone(),
                    mgr.cookies.clone(),
                )
            };

            if cancelled.load(Ordering::SeqCst) {
                let mut t = task_clone.lock().unwrap_or_else(|p| p.into_inner());
                t.status = TaskStatus::Cancelled;
                Self::on_task_finished(&mgr_clone);
                return;
            }

            let task_id_str = task_id.to_string();

            let run_download_attempt = |cookies: Option<&str>| {
                let mut spawned_pid: Option<u32> = None;
                let res = download_media_background_full(
                    &url,
                    &format_id,
                    &yt_dlp_path,
                    &ffmpeg_path,
                    &download_dir,
                    custom_output_template.as_deref(),
                    force_overwrite,
                    Some(&task_id_str),
                    cookies,
                    |child| {
                        let pid = child.id();
                        if cancelled.load(Ordering::SeqCst) {
                            kill_process_by_pid(pid);
                            return;
                        }
                        spawned_pid = Some(pid);
                        let mut mgr = mgr_clone.lock().unwrap_or_else(|p| p.into_inner());
                        if mgr.cancelled.load(Ordering::SeqCst) {
                            kill_process_by_pid(pid);
                        } else {
                            mgr.active_pids.push(pid);
                        }
                    },
                    |percent,
                     downloaded_bytes,
                     total_bytes,
                     speed_bytes_sec,
                     eta_seconds,
                     phase,
                     is_finalizing| {
                        Self::update_task_progress(
                            &task_clone,
                            percent,
                            downloaded_bytes,
                            total_bytes,
                            speed_bytes_sec,
                            eta_seconds,
                            phase,
                            is_finalizing,
                        );
                    },
                );

                if let Some(pid) = spawned_pid {
                    let mut mgr = mgr_clone.lock().unwrap_or_else(|p| p.into_inner());
                    mgr.active_pids.retain(|&p| p != pid);
                }

                res
            };

            let mut result = run_download_attempt(task_cookies.as_deref());

            if !cancelled.load(Ordering::SeqCst)
                && task_cookies.is_none()
                && mgr_cookies.is_some()
                && matches!(&result, Err(e) if is_auth_or_bot_error(e) || is_rate_limit_error(e))
            {
                if let Err(ref e) = result {
                    if is_rate_limit_error(e) {
                        push_notification(
                            format!(
                                "WARNING: Task #{}: Rate limited by platform (HTTP 429). Waiting 1.5s before retrying with cookies...",
                                task_id
                            )
                            .yellow()
                            .to_string(),
                        );
                        for _ in 0..15 {
                            if cancelled.load(Ordering::SeqCst) {
                                break;
                            }
                            std::thread::sleep(Duration::from_millis(100));
                        }
                    } else {
                        push_notification(
                            format!(
                                "INFO: Task #{}: Stream requires authentication. Retrying with session cookies...",
                                task_id
                            )
                            .cyan()
                            .to_string(),
                        );
                    }
                }

                if !cancelled.load(Ordering::SeqCst) {
                    result = run_download_attempt(mgr_cookies.as_deref());

                    if result.is_ok() {
                        let mut t = task_clone.lock().unwrap_or_else(|p| p.into_inner());
                        t.cookies = mgr_cookies.clone();
                    }
                }
            }

            let is_cancelled = cancelled.load(Ordering::SeqCst);

            let (title, download_dir) = {
                let mut t = task_clone.lock().unwrap_or_else(|p| p.into_inner());
                if is_cancelled {
                    t.status = TaskStatus::Cancelled;
                } else {
                    match &result {
                        Ok(captured_path) => {
                            t.status = TaskStatus::Completed;
                            t.progress.percent = 100.0;
                            t.final_filepath = captured_path
                                .clone()
                                .or_else(|| find_completed_file(&t.download_dir, &t.title));
                        }
                        Err(e) => {
                            t.status = TaskStatus::Failed(e.clone());
                        }
                    }
                }
                (t.title.clone(), t.download_dir.clone())
            };

            let temp_dir = std::path::Path::new(&download_dir).join(format!(".dwdr_{}", task_id));
            let _ = std::fs::remove_dir_all(&temp_dir);

            if !is_cancelled {
                match result {
                    Ok(_) => {
                        push_notification(format!(
                            "\n{}",
                            format!(
                                "SUCCESS: Download completed: \"{}\" -> saved to {}",
                                title, download_dir
                            )
                            .green()
                            .bold()
                        ));
                    }
                    Err(e) => {
                        push_notification(format!(
                            "\n{}",
                            format!("ERROR: Download failed for \"{}\": {}", title, e)
                                .red()
                                .bold()
                        ));
                        if let Some(msg) = check_core_engines_update(&yt_dlp_path, &ffmpeg_path) {
                            push_notification(format!("{}\n", msg.yellow()));
                        }
                    }
                }
            }

            Self::on_task_finished(&mgr_clone);
        });

        let mut mgr = manager_arc.lock().unwrap_or_else(|p| p.into_inner());
        mgr.worker_handles.retain(|h| !h.is_finished());
        mgr.worker_handles.push(handle);
    }

    fn on_task_finished(manager_arc: &Arc<Mutex<Self>>) {
        let (next_task, auto_spawn) = {
            let mut mgr = manager_arc.lock().unwrap_or_else(|p| p.into_inner());
            if mgr.active_count > 0 {
                mgr.active_count -= 1;
            }

            if mgr.cancelled.load(Ordering::SeqCst) {
                return;
            }

            let task_info = if mgr.active_count < mgr.max_parallel {
                if let Some(next_id) = mgr.queue.pop_front() {
                    let task_opt = mgr
                        .tasks
                        .iter()
                        .find(|t| t.lock().unwrap_or_else(|p| p.into_inner()).id == next_id)
                        .cloned();
                    if let Some(task) = task_opt {
                        mgr.active_count += 1;
                        task.lock().unwrap_or_else(|p| p.into_inner()).status =
                            TaskStatus::Downloading;
                        Some((task, mgr.active_count, mgr.queue.len(), mgr.max_parallel))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            (task_info, mgr.auto_spawn)
        };

        if let Some((next_task, active, queued, max_parallel)) = next_task {
            let title = next_task
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .title
                .clone();
            push_notification(format!(
                "{}",
                format!(
                    "INFO: Starting queued download: \"{}\" [Active: {}/{}, Queued: {}]",
                    title, active, max_parallel, queued
                )
                .cyan()
            ));
            if auto_spawn {
                record_active_download();
                Self::spawn_worker(manager_arc, next_task);
            }
        } else {
            let is_now_idle = {
                let mgr = manager_arc.lock().unwrap_or_else(|p| p.into_inner());
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

        // Terminate all active processes first to prevent dying processes from recreating partial files.
        for &pid in &self.active_pids {
            kill_process_by_pid(pid);
        }
        self.active_pids.clear();

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
                let temp_dir =
                    std::path::Path::new(&t.download_dir).join(format!(".dwdr_{}", t.id));
                let _ = std::fs::remove_dir_all(&temp_dir);
            }
        }

        self.active_count = 0;
        clear_active_download();
    }

    /// Cancels queued and active tasks, terminates child process trees, and waits for all worker threads to cleanly exit.
    pub fn cancel_and_join(manager_arc: &Arc<Mutex<Self>>) {
        let handles = {
            let mut mgr = match manager_arc.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            mgr.cancel_all();
            std::mem::take(&mut mgr.worker_handles)
        };
        for handle in handles {
            let _ = handle.join();
        }
    }

    /// Waits for all currently running worker threads to finish.
    pub fn join_workers(manager_arc: &Arc<Mutex<Self>>) {
        loop {
            let handles = {
                let mut mgr = match manager_arc.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                if !mgr.is_active() && mgr.worker_handles.is_empty() {
                    break;
                }
                std::mem::take(&mut mgr.worker_handles)
            };
            if handles.is_empty() {
                thread::sleep(Duration::from_millis(20));
                continue;
            }
            for handle in handles {
                let _ = handle.join();
            }
        }
    }

    pub fn is_active(&self) -> bool {
        self.active_count > 0 || !self.queue.is_empty()
    }

    fn render_status_snapshot(tasks: &[Arc<Mutex<DownloadTask>>], max_parallel: usize) {
        let tasks_snapshot: Vec<(usize, String, String, TaskStatus, TaskProgress)> = tasks
            .iter()
            .map(|t| {
                let guard = t.lock().unwrap_or_else(|p| p.into_inner());
                (
                    guard.id,
                    guard.title.clone(),
                    guard.format_desc.clone(),
                    guard.status.clone(),
                    guard.progress.clone(),
                )
            })
            .collect();

        let active_tasks: Vec<_> = tasks_snapshot
            .iter()
            .filter(|(_, _, _, s, _)| matches!(s, TaskStatus::Downloading | TaskStatus::Finalizing))
            .collect();

        let queued_tasks: Vec<_> = tasks_snapshot
            .iter()
            .filter(|(_, _, _, s, _)| matches!(s, TaskStatus::Queued))
            .collect();

        let failed_tasks: Vec<_> = tasks_snapshot
            .iter()
            .filter_map(|(id, title, desc, s, _)| match s {
                TaskStatus::Failed(err) => Some((*id, title.as_str(), desc.as_str(), err.as_str())),
                _ => None,
            })
            .collect();

        let completed_tasks: Vec<_> = tasks_snapshot
            .iter()
            .filter_map(|(id, title, desc, s, _)| match s {
                TaskStatus::Completed => Some((*id, title.as_str(), desc.as_str())),
                _ => None,
            })
            .collect();

        println!(
            "\n{}",
            format!(
                "=== Active Downloads ({}/{}) ===",
                active_tasks.len(),
                max_parallel
            )
            .bold()
        );

        if active_tasks.is_empty() {
            println!("  (None)");
        } else {
            for (id, title, format_desc, status, prog) in &active_tasks {
                let available_bar_width = get_terminal_width().saturating_sub(7);
                let bar = render_progress_bar_with_width(
                    prog.percent,
                    &prog.downloaded_str,
                    &prog.total_str,
                    &prog.speed_str,
                    &prog.eta_str,
                    available_bar_width,
                );
                let bar_clean = bar.trim_start_matches('\r').trim_end();
                let status_str = if *status == TaskStatus::Finalizing {
                    " [Finalizing media...]".yellow()
                } else {
                    "".normal()
                };
                println!(
                    "  [#{}] \"{}\" [{}] {}",
                    id,
                    sanitize_display(title),
                    sanitize_display(format_desc),
                    status_str
                );
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
            for (i, (id, title, format_desc, _, _)) in queued_tasks.iter().enumerate() {
                println!(
                    "  [#{}] (Position: {}) \"{}\" [{}]",
                    id,
                    i + 1,
                    sanitize_display(title),
                    sanitize_display(format_desc)
                );
            }
        }

        if !failed_tasks.is_empty() {
            println!(
                "\n{}",
                format!("=== Failed Downloads ({}) ===", failed_tasks.len())
                    .red()
                    .bold()
            );
            for (id, title, format_desc, err) in &failed_tasks {
                println!(
                    "  [#{}] \"{}\" [{}]\n       Error: {}",
                    id,
                    sanitize_display(title),
                    sanitize_display(format_desc),
                    sanitize_display(err)
                );
            }
        }

        if !completed_tasks.is_empty() {
            println!(
                "\n{}",
                format!("=== Completed Downloads ({}) ===", completed_tasks.len())
                    .green()
                    .bold()
            );
            for (id, title, format_desc) in &completed_tasks {
                println!(
                    "  [#{}] \"{}\" [{}]",
                    id,
                    sanitize_display(title),
                    sanitize_display(format_desc)
                );
            }
        }
        println!();
    }

    pub fn print_status(&self) {
        Self::render_status_snapshot(&self.tasks, self.max_parallel);
    }

    pub fn render_status_frame(
        snapshot: &StatusSnapshot,
        width: usize,
        theme: &ThemeMode,
        color: ColorLevel,
        tick: u64,
    ) -> Vec<String> {
        let mut lines = Vec::new();
        let speed_part = if snapshot.total_speed > 0.0 {
            format!(" | Total Speed: {}", format_speed(snapshot.total_speed))
        } else {
            String::new()
        };
        let header_raw = format!(
            "Active Downloads: {}/{} | Queued in line: {}{}",
            snapshot.active_tasks.len(),
            snapshot.max_parallel,
            snapshot.queued_count,
            speed_part
        );
        lines.push(header_raw);

        for task in &snapshot.active_tasks {
            let title_clean = sanitize_display(&task.title);
            let desc_clean = sanitize_display(&task.format_desc);
            let desc_tag = format!(" [{}]", desc_clean);
            let status_str = if task.status == TaskStatus::Finalizing {
                match color {
                    ColorLevel::None => " [Finalizing media...]".to_string(),
                    _ => " [Finalizing media...]".yellow().to_string(),
                }
            } else {
                String::new()
            };

            let prefix = format!("  [#{}] \"", task.id);
            let suffix = format!("\"{}{}", desc_tag, status_str);
            let fixed_w = visible_width(&prefix) + visible_width(&suffix);
            let max_title_w = width.saturating_sub(fixed_w).max(5);
            let title_display = truncate_display(&title_clean, max_title_w);
            lines.push(format!("{}{}{}", prefix, title_display, suffix));

            const STATUS_INDENT: usize = 7;
            let bar_width = width.saturating_sub(STATUS_INDENT);
            let state = ProgressState {
                percent: task.progress.percent,
                downloaded_bytes: task.progress.downloaded_bytes,
                total_bytes: task.progress.total_bytes,
                speed_bytes_sec: task.progress.speed_bytes_sec,
                eta_seconds: task.progress.eta_seconds,
                sparkline_samples: task.progress.sparkline_samples.clone(),
                phase: task.progress.phase,
                is_finalizing: task.status == TaskStatus::Finalizing,
                finalizing_msg: if task.status == TaskStatus::Finalizing {
                    Some("Finalizing media...".to_string())
                } else {
                    None
                },
            };
            let is_audio_only = task.format_desc.starts_with("Audio");
            let show_stepper = !is_audio_only && task.status != TaskStatus::Finalizing;
            let frame =
                render_foreground_frame(&state, bar_width, theme, color, tick, show_stepper);
            lines.push(format!("       {}", frame));
        }

        lines
    }

    pub fn render_live_status_lines(
        active_tasks: &[(usize, String, String, TaskStatus, TaskProgress)],
        max_parallel: usize,
        queued_count: usize,
        term_w: usize,
    ) -> Vec<String> {
        let tasks = active_tasks
            .iter()
            .map(|(id, title, format_desc, status, prog)| TaskSnapshot {
                id: *id,
                title: title.clone(),
                format_desc: format_desc.clone(),
                status: status.clone(),
                progress: prog.clone(),
            })
            .collect();
        let snapshot = StatusSnapshot {
            active_tasks: tasks,
            max_parallel,
            queued_count,
            total_speed: 0.0,
        };
        Self::render_status_frame(&snapshot, term_w, &ThemeMode::Ascii, ColorLevel::None, 0)
    }

    pub fn print_status_live(manager_arc: &Arc<Mutex<Self>>) {
        let (tasks, max_parallel, is_active) = {
            let mgr = manager_arc.lock().unwrap_or_else(|p| p.into_inner());
            (mgr.tasks.clone(), mgr.max_parallel, mgr.is_active())
        };

        #[cfg(test)]
        {
            let _ = is_active;
            Self::render_status_snapshot(&tasks, max_parallel);
        }

        #[cfg(not(test))]
        {
            if !io::stdout().is_terminal() || !io::stdin().is_terminal() {
                Self::render_status_snapshot(&tasks, max_parallel);
                println!();
                return;
            }

            if !is_active {
                println!("\n{}", "INFO: No active downloads in progress.".cyan());
                Self::render_status_snapshot(&tasks, max_parallel);
                println!();
                return;
            }

            println!(
                "\n{}",
                "=== Real-Time Download Status (Press 'q' or Enter to exit live view) ===".bold()
            );

            let mut lines_printed: u16 = 0;
            let mut prev_frame = String::new();
            let mut tick: u64 = 0;
            let mut confirming_cancel = false;
            let mut raw_guard = RawModeGuard::new();

            // Discard any pending keystrokes (e.g. Enter pressed to select format or submit URL).
            for _ in 0..64 {
                if let Ok(true) = crossterm::event::poll(Duration::from_millis(0)) {
                    let _ = crossterm::event::read();
                } else {
                    break;
                }
            }

            loop {
                if let Ok(true) = crossterm::event::poll(Duration::from_millis(TICK_MS))
                    && let Ok(crossterm::event::Event::Key(key_event)) = crossterm::event::read()
                    && key_event.kind != crossterm::event::KeyEventKind::Release
                {
                    match handle_live_key(key_event.code, confirming_cancel) {
                        LiveAction::Detach => break,
                        LiveAction::AskCancel => {
                            confirming_cancel = true;
                        }
                        LiveAction::ConfirmCancel => {
                            Self::cancel_and_join(manager_arc);
                            break;
                        }
                        LiveAction::DismissCancel => {
                            confirming_cancel = false;
                        }
                        LiveAction::None => {}
                    }
                }

                let (active_tasks, queued_count, total_speed) = {
                    let mut active = Vec::new();
                    let mut queued = 0;
                    let mut spd = 0.0;
                    for t in &tasks {
                        let guard = t.lock().unwrap_or_else(|p| p.into_inner());
                        match guard.status {
                            TaskStatus::Downloading | TaskStatus::Finalizing => {
                                let mut progress = guard.progress.clone();
                                if guard.status == TaskStatus::Downloading {
                                    spd += progress.speed_bytes_sec;
                                } else {
                                    progress.speed_bytes_sec = 0.0;
                                }
                                active.push(TaskSnapshot {
                                    id: guard.id,
                                    title: guard.title.clone(),
                                    format_desc: guard.format_desc.clone(),
                                    status: guard.status.clone(),
                                    progress,
                                });
                            }
                            TaskStatus::Queued => {
                                queued += 1;
                            }
                            _ => {}
                        }
                    }
                    (active, queued, spd)
                };

                if active_tasks.is_empty() && queued_count == 0 {
                    raw_guard.restore();
                    if lines_printed > 0 {
                        let _ = crossterm::execute!(
                            io::stdout(),
                            crossterm::cursor::MoveUp(lines_printed),
                            crossterm::cursor::MoveToColumn(0),
                            crossterm::terminal::Clear(
                                crossterm::terminal::ClearType::FromCursorDown
                            )
                        );
                    }

                    let mut failed_tasks = Vec::new();
                    let mut completed_tasks = Vec::new();
                    let mut has_cancelled = false;

                    for t in &tasks {
                        let guard = t.lock().unwrap_or_else(|p| p.into_inner());
                        match &guard.status {
                            TaskStatus::Failed(err) => {
                                failed_tasks.push((
                                    guard.id,
                                    guard.title.clone(),
                                    guard.format_desc.clone(),
                                    err.clone(),
                                ));
                            }
                            TaskStatus::Completed => {
                                completed_tasks.push((
                                    guard.id,
                                    guard.title.clone(),
                                    guard.format_desc.clone(),
                                ));
                            }
                            TaskStatus::Cancelled => {
                                has_cancelled = true;
                            }
                            _ => {}
                        }
                    }

                    if !failed_tasks.is_empty() {
                        for (id, title, desc, err) in &failed_tasks {
                            eprintln!(
                                "{}",
                                format!(
                                    "\nERROR: Download failed for [#{}] \"{}\" [{}]: {}\n",
                                    id,
                                    sanitize_display(title),
                                    sanitize_display(desc),
                                    sanitize_display(err)
                                )
                                .red()
                                .bold()
                            );
                        }
                    }

                    if !completed_tasks.is_empty() {
                        if failed_tasks.is_empty() {
                            println!("{}", "SUCCESS: All active tasks completed!".green());
                        } else {
                            println!(
                                "{}",
                                format!(
                                    "SUCCESS: {} of {} task(s) completed successfully.",
                                    completed_tasks.len(),
                                    completed_tasks.len() + failed_tasks.len()
                                )
                                .green()
                            );
                        }
                    } else if failed_tasks.is_empty() && has_cancelled {
                        println!("{}", "INFO: Tasks were cancelled.".yellow());
                    }

                    lines_printed = 0;
                    break;
                }

                set_terminal_title(&format!(
                    "DWDR Live - Active: {}/{} - Queued: {}",
                    active_tasks.len(),
                    max_parallel,
                    queued_count
                ));

                let term_w = UiContext::current_width();
                let ctx = UiContext::get();
                let snapshot = StatusSnapshot {
                    active_tasks,
                    max_parallel,
                    queued_count,
                    total_speed,
                };

                let status_lines =
                    Self::render_status_frame(&snapshot, term_w, &ctx.theme, ctx.color_level, tick);

                let mut current_frame = status_lines.join("\n");
                if confirming_cancel {
                    current_frame.push_str("\n  Cancel all downloads? [y/N] ");
                } else {
                    current_frame.push_str("\n  [q] Detach to prompt | [c] Cancel tasks");
                }

                if current_frame == prev_frame {
                    tick = tick.wrapping_add(1);
                    continue;
                }
                prev_frame = current_frame.clone();

                if lines_printed > 0 {
                    let _ = crossterm::execute!(
                        io::stdout(),
                        crossterm::cursor::MoveUp(lines_printed),
                        crossterm::cursor::MoveToColumn(0),
                        crossterm::terminal::Clear(crossterm::terminal::ClearType::FromCursorDown)
                    );
                }

                let use_sync = ctx.is_tty && ctx.color_level != ColorLevel::None;
                if use_sync {
                    print!("\x1b[?2026h");
                }
                print!("{}\r\n", current_frame.replace('\n', "\r\n"));
                if use_sync {
                    print!("\x1b[?2026l");
                }
                let _ = io::stdout().flush();
                lines_printed = (status_lines.len() + 1) as u16;
                tick = tick.wrapping_add(1);
            }

            raw_guard.restore();
            set_terminal_title("DWDR");

            if lines_printed > 0 {
                let _ = crossterm::execute!(
                    io::stdout(),
                    crossterm::cursor::MoveUp(lines_printed),
                    crossterm::cursor::MoveToColumn(0),
                    crossterm::terminal::Clear(crossterm::terminal::ClearType::FromCursorDown)
                );
            }
            println!("{}", "INFO: Exited real-time status monitor.".cyan());
            Self::render_status_snapshot(&tasks, max_parallel);
            println!(
                "{}",
                "Tip: Type 'status' to reopen live view, or 'queue' to inspect pending queue."
                    .cyan()
            );
            println!();
        }
    }

    pub fn print_list(&self) {
        let tasks_snapshot: Vec<_> = self
            .tasks
            .iter()
            .map(|t| {
                let guard = t.lock().unwrap_or_else(|p| p.into_inner());
                (
                    guard.id,
                    guard.title.clone(),
                    guard.format_desc.clone(),
                    guard.download_dir.clone(),
                    guard.status.clone(),
                    guard.progress.clone(),
                    guard.final_filepath.clone(),
                )
            })
            .collect();

        if tasks_snapshot.is_empty() {
            println!(
                "\n{}\n",
                "INFO: No downloads recorded in this session.".cyan()
            );
            println!(
                "{}",
                "Tip: Enter a media URL to start a download, or type 'help' for available commands.".cyan()
            );
            return;
        }

        println!("\n{}", "=== Download History & Session Report ===".bold());
        for (id, title, format_desc, dir, status, prog, final_filepath) in &tasks_snapshot {
            match status {
                TaskStatus::Downloading => {
                    println!(
                        "  [#{}] \"{}\" [{}] - {} ({:.1}%)",
                        id,
                        sanitize_display(title),
                        sanitize_display(format_desc),
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
                        "  [#{}] \"{}\" [{}] - {}",
                        id,
                        sanitize_display(title),
                        sanitize_display(format_desc),
                        "FINALIZING MEDIA".yellow().bold()
                    );
                }
                TaskStatus::Queued => {
                    println!(
                        "  [#{}] \"{}\" [{}] - {}",
                        id,
                        sanitize_display(title),
                        sanitize_display(format_desc),
                        "QUEUED".blue()
                    );
                }
                TaskStatus::Completed => {
                    let resolved_path = final_filepath
                        .clone()
                        .or_else(|| find_completed_file(dir, title));
                    if let Some(path) = resolved_path {
                        println!(
                            "  [#{}] \"{}\" [{}] - {} -> {}",
                            id,
                            sanitize_display(title),
                            sanitize_display(format_desc),
                            "COMPLETED".green().bold(),
                            path.display()
                        );
                    } else {
                        println!(
                            "  [#{}] \"{}\" [{}] - {} -> {} (file not found)",
                            id,
                            sanitize_display(title),
                            sanitize_display(format_desc),
                            "COMPLETED".yellow().bold(),
                            dir
                        );
                        println!(
                            "       {}",
                            "Tip: The media file may have been moved, renamed, or deleted from the download directory.".cyan()
                        );
                    }
                }
                TaskStatus::Failed(err) => {
                    println!(
                        "  [#{}] \"{}\" [{}] - {} ({})",
                        id,
                        sanitize_display(title),
                        sanitize_display(format_desc),
                        "FAILED".red().bold(),
                        sanitize_display(err)
                    );
                }
                TaskStatus::Cancelled => {
                    println!(
                        "  [#{}] \"{}\" [{}] - {}",
                        id,
                        sanitize_display(title),
                        sanitize_display(format_desc),
                        "CANCELLED".yellow()
                    );
                }
            }
        }
        println!(
            "{}",
            "Tip: Type 'status' for real-time progress of active tasks, or enter a URL to download.".cyan()
        );
        println!();
    }

    fn render_queue_snapshot(tasks: &[Arc<Mutex<DownloadTask>>], max_parallel: usize) {
        let tasks_snapshot: Vec<(usize, String, String, TaskStatus)> = tasks
            .iter()
            .map(|t| {
                let guard = t.lock().unwrap_or_else(|p| p.into_inner());
                (
                    guard.id,
                    guard.title.clone(),
                    guard.format_desc.clone(),
                    guard.status.clone(),
                )
            })
            .collect();

        let queued_tasks: Vec<_> = tasks_snapshot
            .iter()
            .filter(|(_, _, _, s)| matches!(s, TaskStatus::Queued))
            .collect();

        let active_count = tasks_snapshot
            .iter()
            .filter(|(_, _, _, s)| matches!(s, TaskStatus::Downloading | TaskStatus::Finalizing))
            .count();

        println!(
            "\n{}",
            format!(
                "=== Download Queue (Active Slots: {}/{}, Queued in Line: {}) ===",
                active_count,
                max_parallel,
                queued_tasks.len()
            )
            .bold()
        );

        if queued_tasks.is_empty() {
            println!("  (Queue is empty)");
            println!(
                "  {}",
                "Tip: Enter a media URL to start a download, or type 'help' for available commands.".cyan()
            );
        } else {
            for (i, (id, title, format_desc, _)) in queued_tasks.iter().enumerate() {
                println!(
                    "  [#{}] Position #{}: \"{}\" [{}]",
                    id,
                    i + 1,
                    sanitize_display(title),
                    sanitize_display(format_desc)
                );
            }
            println!(
                "  {}",
                "Tip: Type 'queue live' to watch queue positions advance in real time.".cyan()
            );
        }
        println!();
    }

    pub fn print_queue(&self) {
        Self::render_queue_snapshot(&self.tasks, self.max_parallel);
    }

    pub fn print_queue_live(manager_arc: &Arc<Mutex<Self>>) {
        let (tasks, max_parallel, is_active) = {
            let mgr = manager_arc.lock().unwrap_or_else(|p| p.into_inner());
            (mgr.tasks.clone(), mgr.max_parallel, mgr.is_active())
        };

        #[cfg(test)]
        {
            let _ = is_active;
            Self::render_queue_snapshot(&tasks, max_parallel);
        }

        #[cfg(not(test))]
        {
            if !io::stdout().is_terminal() || !io::stdin().is_terminal() {
                Self::render_queue_snapshot(&tasks, max_parallel);
                return;
            }

            if !is_active {
                println!(
                    "\n{}",
                    "INFO: No queued or active downloads in progress.".cyan()
                );
                Self::render_queue_snapshot(&tasks, max_parallel);
                return;
            }

            println!(
                "\n{}",
                "=== Real-Time Download Queue Monitor (Press 'q' or Enter to exit live view) ==="
                    .bold()
            );

            let mut lines_printed: u16 = 0;
            let mut raw_guard = RawModeGuard::new();

            // Discard any pending keystrokes so stale inputs do not prematurely detach.
            for _ in 0..64 {
                if let Ok(true) = crossterm::event::poll(Duration::from_millis(0)) {
                    let _ = crossterm::event::read();
                } else {
                    break;
                }
            }

            loop {
                if let Ok(true) = crossterm::event::poll(Duration::from_millis(200))
                    && let Ok(crossterm::event::Event::Key(key_event)) = crossterm::event::read()
                    && key_event.kind != crossterm::event::KeyEventKind::Release
                {
                    match key_event.code {
                        crossterm::event::KeyCode::Char('q')
                        | crossterm::event::KeyCode::Char('Q')
                        | crossterm::event::KeyCode::Enter
                        | crossterm::event::KeyCode::Esc => break,
                        _ => {}
                    }
                }

                let mut active_tasks = Vec::new();
                let mut queued_tasks = Vec::new();

                for t in &tasks {
                    let guard = t.lock().unwrap_or_else(|p| p.into_inner());
                    match guard.status {
                        TaskStatus::Downloading | TaskStatus::Finalizing => {
                            active_tasks.push((
                                guard.id,
                                guard.title.clone(),
                                guard.format_desc.clone(),
                                guard.status.clone(),
                            ));
                        }
                        TaskStatus::Queued => {
                            queued_tasks.push((
                                guard.id,
                                guard.title.clone(),
                                guard.format_desc.clone(),
                                guard.status.clone(),
                            ));
                        }
                        _ => {}
                    }
                }

                if lines_printed > 0 {
                    let _ = crossterm::execute!(
                        io::stdout(),
                        crossterm::cursor::MoveUp(lines_printed),
                        crossterm::cursor::MoveToColumn(0),
                        crossterm::terminal::Clear(crossterm::terminal::ClearType::FromCursorDown)
                    );
                }

                if active_tasks.is_empty() && queued_tasks.is_empty() {
                    raw_guard.restore();
                    println!(
                        "{}",
                        "SUCCESS: All active and queued downloads finished!".green()
                    );
                    lines_printed = 0;
                    break;
                }

                let mut frame_lines = Vec::new();
                frame_lines.push(format!(
                    "Active Slots: {}/{} | Queued in line: {}",
                    active_tasks.len(),
                    max_parallel,
                    queued_tasks.len()
                ));

                if !active_tasks.is_empty() {
                    frame_lines.push(format!("  {}", "Currently Downloading:".cyan()));
                    for (id, title, format_desc, status) in &active_tasks {
                        let status_tag = if *status == TaskStatus::Finalizing {
                            " [Finalizing media...]".yellow()
                        } else {
                            " [Downloading]".green()
                        };
                        frame_lines.push(format!(
                            "    [#{}] \"{}\" [{}] {}",
                            id,
                            sanitize_display(title),
                            sanitize_display(format_desc),
                            status_tag
                        ));
                    }
                }

                if !queued_tasks.is_empty() {
                    frame_lines.push(format!("  {}", "Pending Queue:".blue()));
                    for (i, (id, title, format_desc, _)) in queued_tasks.iter().enumerate() {
                        frame_lines.push(format!(
                            "    [#{}] Position #{}: \"{}\" [{}]",
                            id,
                            i + 1,
                            sanitize_display(title),
                            sanitize_display(format_desc)
                        ));
                    }
                }

                print!("{}\r\n", frame_lines.join("\r\n"));
                let _ = io::stdout().flush();
                lines_printed = frame_lines.len() as u16;
            }

            raw_guard.restore();

            if lines_printed > 0 {
                let _ = crossterm::execute!(
                    io::stdout(),
                    crossterm::cursor::MoveUp(lines_printed),
                    crossterm::cursor::MoveToColumn(0),
                    crossterm::terminal::Clear(crossterm::terminal::ClearType::FromCursorDown)
                );
            }
            println!("{}", "INFO: Exited real-time queue monitor.".cyan());
            Self::render_queue_snapshot(&tasks, max_parallel);
            println!();
        }
    }
}

/// Locates a completed media file on disk matching the title within the download directory.
pub fn find_completed_file(download_dir: &str, title: &str) -> Option<PathBuf> {
    let dir_path = std::path::Path::new(download_dir);
    if !dir_path.is_dir() {
        return None;
    }

    let sanitized_title = sanitize_media_title(title);
    if sanitized_title.is_empty() {
        return None;
    }

    let legacy_sanitized: String = title
        .chars()
        .filter(|c| !['/', '\\', ':', '*', '?', '"', '<', '>', '|'].contains(c))
        .collect::<String>()
        .trim()
        .to_string();

    let normalize_parens = |s: &str| -> String {
        s.replace(['(', ')', '[', ']', '{', '}'], "")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let norm_title = normalize_parens(&sanitized_title).to_lowercase();
    let norm_legacy = normalize_parens(&legacy_sanitized).to_lowercase();

    let read_dir = std::fs::read_dir(dir_path).ok()?;
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_file()
            && let Some(file_name) = path.file_name().and_then(|n| n.to_str())
        {
            let name_lower = file_name.to_lowercase();
            if name_lower.ends_with(".part")
                || name_lower.ends_with(".ytdl")
                || name_lower.ends_with(".temp")
                || name_lower.ends_with(".tmp")
            {
                continue;
            }

            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let norm_stem = normalize_parens(stem).to_lowercase();

            // Eliminates fuzzy word subset matching to prevent false positives like 'Lecture 1' matching 'Lecture 10.mp4'.
            if stem.eq_ignore_ascii_case(&sanitized_title)
                || stem.eq_ignore_ascii_case(&legacy_sanitized)
                || stem.starts_with(&format!("{} (", sanitized_title))
                || stem.starts_with(&format!("{} (", legacy_sanitized))
                || norm_stem == norm_title
                || norm_stem == norm_legacy
            {
                return Some(path);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    #[cfg(unix)]
    use std::io::{BufRead, BufReader};
    #[cfg(unix)]
    use std::os::unix::process::CommandExt;
    #[cfg(unix)]
    use std::process::Stdio;

    #[test]
    fn test_queue_enqueue_and_capacity() {
        let mgr_arc = QueueManager::new(2, PathBuf::from("yt-dlp"), PathBuf::from("ffmpeg"));
        mgr_arc.lock().unwrap().auto_spawn = false;
        assert_eq!(mgr_arc.lock().unwrap().max_parallel, 2);
        assert!(!mgr_arc.lock().unwrap().is_active());

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
        mgr_arc.lock().unwrap().auto_spawn = false;
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
        assert!(matches!(
            mgr.tasks[0].lock().unwrap().status,
            TaskStatus::Cancelled
        ));
        assert!(matches!(
            mgr.tasks[1].lock().unwrap().status,
            TaskStatus::Cancelled
        ));
    }

    #[test]
    #[serial]
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
        mgr_arc.lock().unwrap().auto_spawn = false;
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
            format_desc: "1080p (mp4, 45 MiB)".to_string(),
            download_dir: "/downloads".to_string(),
            status: TaskStatus::Downloading,
            progress: TaskProgress {
                percent: 45.5,
                downloaded_str: "45 MiB".to_string(),
                total_str: "100 MiB".to_string(),
                speed_str: "2.5 MiB/s".to_string(),
                eta_str: "00:22".to_string(),
                ..TaskProgress::default()
            },
            custom_output_template: None,
            force_overwrite: false,
            cookies: None,
            final_filepath: None,
            smoother: SpeedSmoother::default(),
            last_sample_time: None,
        })));

        mgr.tasks.push(Arc::new(Mutex::new(DownloadTask {
            id: 2,
            url: "https://example.com/2".to_string(),
            title: "Finalizing Video".to_string(),
            format_id: "137".to_string(),
            format_desc: "1080p (mp4)".to_string(),
            download_dir: "/downloads".to_string(),
            status: TaskStatus::Finalizing,
            progress: TaskProgress::default(),
            custom_output_template: None,
            force_overwrite: false,
            cookies: None,
            final_filepath: None,
            smoother: SpeedSmoother::default(),
            last_sample_time: None,
        })));

        mgr.tasks.push(Arc::new(Mutex::new(DownloadTask {
            id: 3,
            url: "https://example.com/3".to_string(),
            title: "Queued Video".to_string(),
            format_id: "137".to_string(),
            format_desc: "1080p (mp4)".to_string(),
            download_dir: "/downloads".to_string(),
            status: TaskStatus::Queued,
            progress: TaskProgress::default(),
            custom_output_template: None,
            force_overwrite: false,
            cookies: None,
            final_filepath: None,
            smoother: SpeedSmoother::default(),
            last_sample_time: None,
        })));

        mgr.tasks.push(Arc::new(Mutex::new(DownloadTask {
            id: 4,
            url: "https://example.com/4".to_string(),
            title: "Completed Video".to_string(),
            format_id: "137".to_string(),
            format_desc: "1080p (mp4)".to_string(),
            download_dir: "/downloads".to_string(),
            status: TaskStatus::Completed,
            progress: TaskProgress {
                percent: 100.0,
                downloaded_str: "100 MiB".to_string(),
                total_str: "100 MiB".to_string(),
                speed_str: "0 B/s".to_string(),
                eta_str: "00:00".to_string(),
                ..TaskProgress::default()
            },
            custom_output_template: None,
            force_overwrite: false,
            cookies: None,
            final_filepath: None,
            smoother: SpeedSmoother::default(),
            last_sample_time: None,
        })));

        mgr.tasks.push(Arc::new(Mutex::new(DownloadTask {
            id: 5,
            url: "https://example.com/5".to_string(),
            title: "Failed Video".to_string(),
            format_id: "137".to_string(),
            format_desc: "1080p (mp4)".to_string(),
            download_dir: "/downloads".to_string(),
            status: TaskStatus::Failed("Disk space exhausted".to_string()),
            progress: TaskProgress::default(),
            custom_output_template: None,
            force_overwrite: false,
            cookies: None,
            final_filepath: None,
            smoother: SpeedSmoother::default(),
            last_sample_time: None,
        })));

        mgr.tasks.push(Arc::new(Mutex::new(DownloadTask {
            id: 6,
            url: "https://example.com/6".to_string(),
            title: "Cancelled Video".to_string(),
            format_id: "137".to_string(),
            format_desc: "1080p (mp4)".to_string(),
            download_dir: "/downloads".to_string(),
            status: TaskStatus::Cancelled,
            progress: TaskProgress::default(),
            custom_output_template: None,
            force_overwrite: false,
            cookies: None,
            final_filepath: None,
            smoother: SpeedSmoother::default(),
            last_sample_time: None,
        })));

        mgr.print_list();
        mgr.print_status();
    }

    #[test]
    fn test_queue_print_queue_and_status_live_empty_and_populated() {
        let mgr_arc = QueueManager::new(2, PathBuf::from("yt-dlp"), PathBuf::from("ffmpeg"));
        mgr_arc.lock().unwrap().print_queue();
        QueueManager::print_status_live(&mgr_arc);

        {
            let mut mgr = mgr_arc.lock().unwrap();
            mgr.queue.push_back(10);
            mgr.queue.push_back(11);
            mgr.tasks.push(Arc::new(Mutex::new(DownloadTask {
                id: 10,
                url: "url10".to_string(),
                title: "Queued Task 10".to_string(),
                format_id: "137".to_string(),
                format_desc: "1080p (mp4)".to_string(),
                download_dir: "/tmp".to_string(),
                status: TaskStatus::Queued,
                progress: TaskProgress::default(),
                custom_output_template: None,
                force_overwrite: false,
                cookies: None,
                final_filepath: None,
                smoother: SpeedSmoother::default(),
                last_sample_time: None,
            })));
            mgr.tasks.push(Arc::new(Mutex::new(DownloadTask {
                id: 11,
                url: "url11".to_string(),
                title: "Queued Task 11".to_string(),
                format_id: "137".to_string(),
                format_desc: "1080p (mp4)".to_string(),
                download_dir: "/tmp".to_string(),
                status: TaskStatus::Queued,
                progress: TaskProgress::default(),
                custom_output_template: None,
                force_overwrite: false,
                cookies: None,
                final_filepath: None,
                smoother: SpeedSmoother::default(),
                last_sample_time: None,
            })));

            assert!(mgr.is_active());
            mgr.print_queue();
        }
        QueueManager::print_status_live(&mgr_arc);
    }

    #[test]
    fn test_queue_max_parallel_reduction() {
        let mgr_arc = QueueManager::new(3, PathBuf::from("yt-dlp"), PathBuf::from("ffmpeg"));
        {
            let mut mgr = mgr_arc.lock().unwrap();
            mgr.active_count = 3;
            mgr.max_parallel = 1;
            mgr.queue.push_back(100);
            mgr.tasks.push(Arc::new(Mutex::new(DownloadTask {
                id: 100,
                url: "url100".to_string(),
                title: "Queued Task 100".to_string(),
                format_id: "137".to_string(),
                format_desc: "1080p (mp4)".to_string(),
                download_dir: "/tmp".to_string(),
                status: TaskStatus::Queued,
                progress: TaskProgress::default(),
                custom_output_template: None,
                force_overwrite: false,
                cookies: None,
                final_filepath: None,
                smoother: SpeedSmoother::default(),
                last_sample_time: None,
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
        mgr_arc.lock().unwrap().auto_spawn = false;
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

    #[test]
    fn test_queue_worker_threads_joined_on_cancel() {
        let mgr_arc = QueueManager::new(2, PathBuf::from("yt-dlp"), PathBuf::from("ffmpeg"));
        let finished = Arc::new(AtomicBool::new(false));
        let finished_clone = Arc::clone(&finished);

        let handle = thread::spawn(move || {
            thread::sleep(std::time::Duration::from_millis(50));
            finished_clone.store(true, Ordering::SeqCst);
        });

        mgr_arc.lock().unwrap().worker_handles.push(handle);
        assert_eq!(mgr_arc.lock().unwrap().worker_handles.len(), 1);

        QueueManager::cancel_and_join(&mgr_arc);
        assert!(finished.load(Ordering::SeqCst));
        assert!(mgr_arc.lock().unwrap().worker_handles.is_empty());
    }

    #[test]
    fn test_queue_immediate_cancel_after_enqueue() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mgr_arc = QueueManager::new(2, PathBuf::from("yt-dlp"), PathBuf::from("ffmpeg"));
        mgr_arc.lock().unwrap().auto_spawn = false;

        let res = QueueManager::enqueue(
            &mgr_arc,
            "https://example.com/fast".to_string(),
            "Immediate Cancel Video".to_string(),
            "137".to_string(),
            temp_dir.path().to_str().unwrap().to_string(),
        );
        assert!(matches!(res, EnqueueResult::Started { task_id: 1, .. }));

        QueueManager::cancel_and_join(&mgr_arc);

        let mgr = mgr_arc.lock().unwrap();
        assert!(mgr.cancelled.load(Ordering::SeqCst));
        assert!(mgr.queue.is_empty());
        assert_eq!(mgr.active_count, 0);

        let task = mgr.tasks[0].lock().unwrap();
        assert!(matches!(task.status, TaskStatus::Cancelled));
    }

    #[test]
    fn test_queue_find_completed_file_verification() {
        let temp_dir = tempfile::tempdir().unwrap();
        let download_dir = temp_dir.path().to_str().unwrap();

        let title = "Rust Concurrency Tutorial (2026)";
        assert!(find_completed_file(download_dir, title).is_none());

        let completed_path = temp_dir.path().join("Rust Concurrency Tutorial 2026.mp4");
        std::fs::write(&completed_path, b"dummy media bytes").unwrap();

        let found = find_completed_file(download_dir, title);
        assert!(found.is_some());
        assert_eq!(found.unwrap(), completed_path);

        std::fs::remove_file(&completed_path).unwrap();
        assert!(find_completed_file(download_dir, title).is_none());
    }

    #[test]
    fn test_queue_print_queue_live_fallback() {
        let mgr_arc = QueueManager::new(2, PathBuf::from("yt-dlp"), PathBuf::from("ffmpeg"));
        QueueManager::print_queue_live(&mgr_arc);
        mgr_arc.lock().unwrap().print_queue();
    }

    #[test]
    fn test_queue_status_and_queue_live_drop_mutex_guard() {
        let mgr_arc = QueueManager::new(2, PathBuf::from("yt-dlp"), PathBuf::from("ffmpeg"));
        {
            let mut mgr = mgr_arc.lock().unwrap();
            mgr.tasks.push(Arc::new(Mutex::new(DownloadTask {
                id: 1,
                url: "https://example.com/video".to_string(),
                title: "Active Task".to_string(),
                format_id: "137".to_string(),
                format_desc: "1080p".to_string(),
                download_dir: "/tmp".to_string(),
                status: TaskStatus::Downloading,
                progress: TaskProgress::default(),
                custom_output_template: None,
                force_overwrite: false,
                cookies: None,
                final_filepath: None,
                smoother: SpeedSmoother::default(),
                last_sample_time: None,
            })));
            mgr.active_count = 1;
        }

        QueueManager::print_status_live(&mgr_arc);
        assert!(mgr_arc.try_lock().is_ok());

        QueueManager::print_queue_live(&mgr_arc);
        assert!(mgr_arc.try_lock().is_ok());
    }

    #[test]
    fn test_render_live_status_lines_pure_formatting() {
        let active_tasks = vec![
            (
                1,
                "Normal Title Video".to_string(),
                "1080p (mp4)".to_string(),
                TaskStatus::Downloading,
                TaskProgress {
                    percent: 50.0,
                    downloaded_str: "50 MiB".to_string(),
                    total_str: "100 MiB".to_string(),
                    speed_str: "5.0 MiB/s".to_string(),
                    eta_str: "00:10".to_string(),
                    ..TaskProgress::default()
                },
            ),
            (
                2,
                "Finalizing Title Video".to_string(),
                "720p".to_string(),
                TaskStatus::Finalizing,
                TaskProgress::default(),
            ),
        ];

        let lines = QueueManager::render_live_status_lines(&active_tasks, 3, 4, 80);
        assert_eq!(lines.len(), 5);
        assert_eq!(lines[0], "Active Downloads: 2/3 | Queued in line: 4");
        assert!(lines[1].contains("[#1] \"Normal Title Video\" [1080p (mp4)]"));
        assert!(lines[2].contains("50.0%"));
        assert!(lines[3].contains("[#2] \"Finalizing Title Video\" [720p]"));
        assert!(lines[3].contains("Finalizing media..."));
    }

    #[test]
    fn test_render_live_status_lines_unicode_truncation() {
        // Multi-byte wide Japanese title ("Japanese Title Very Long Filename Sample Test...")
        // to verify double-width characters truncate safely without UTF-8 boundary panics.
        let active_tasks = vec![(
            1,
            "日本語タイトル非常に長い動画ファイル名サンプルテストあいうえおかきくけこさしすせそ"
                .to_string(),
            "1080p".to_string(),
            TaskStatus::Downloading,
            TaskProgress::default(),
        )];

        let lines = QueueManager::render_live_status_lines(&active_tasks, 1, 0, 40);
        assert_eq!(lines.len(), 3);
        assert!(lines[1].contains("..."));
    }

    #[test]
    fn test_kill_process_by_pid_zero_noop() {
        kill_process_by_pid(0);
    }

    #[cfg(unix)]
    #[test]
    fn test_kill_process_by_pid_tree_termination() {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("sleep 60 & echo $! && wait")
            .process_group(0)
            .stdout(Stdio::piped())
            .spawn()
            .expect("Failed to spawn process group parent");

        let parent_pid = child.id();
        assert!(parent_pid > 0);

        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let mut child_pid_str = String::new();
        reader.read_line(&mut child_pid_str).unwrap();
        let child_pid: u32 = child_pid_str.trim().parse().unwrap();
        assert!(child_pid > 0);

        let status = Command::new("kill")
            .arg("-0")
            .arg(child_pid.to_string())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());

        kill_process_by_pid(parent_pid);

        let mut terminated = false;
        for _ in 0..40 {
            std::thread::sleep(std::time::Duration::from_millis(25));
            let grandchild_status = Command::new("kill")
                .arg("-0")
                .arg(child_pid.to_string())
                .stderr(Stdio::null())
                .status();
            if grandchild_status.map(|s| !s.success()).unwrap_or(true) {
                terminated = true;
                break;
            }
        }
        assert!(terminated, "Grandchild process should have been terminated");

        let _ = child.wait();
    }

    #[test]
    fn test_render_status_frame_pure() {
        let task1 = TaskSnapshot {
            id: 1,
            title: "Download One".to_string(),
            format_desc: "1080p".to_string(),
            status: TaskStatus::Downloading,
            progress: TaskProgress {
                percent: 65.0,
                downloaded_bytes: 650_000,
                total_bytes: 1_000_000,
                speed_bytes_sec: 150_000.0,
                eta_seconds: Some(2),
                phase: DownloadPhase::Video,
                sparkline_samples: vec![10.0, 20.0, 30.0],
                ..TaskProgress::default()
            },
        };
        let task2 = TaskSnapshot {
            id: 2,
            title: "Download Two".to_string(),
            format_desc: "Audio".to_string(),
            status: TaskStatus::Finalizing,
            progress: TaskProgress {
                percent: 100.0,
                downloaded_bytes: 500_000,
                total_bytes: 500_000,
                speed_bytes_sec: 0.0,
                eta_seconds: None,
                phase: DownloadPhase::Finalize,
                sparkline_samples: Vec::new(),
                ..TaskProgress::default()
            },
        };

        let snapshot = StatusSnapshot {
            active_tasks: vec![task1, task2],
            max_parallel: 2,
            queued_count: 3,
            total_speed: 150_000.0,
        };

        let lines = QueueManager::render_status_frame(
            &snapshot,
            80,
            &ThemeMode::Ascii,
            ColorLevel::None,
            0,
        );

        assert_eq!(lines.len(), 5);
        assert!(lines[0].contains("Active Downloads: 2/2 | Queued in line: 3 | Total Speed:"));
        assert!(lines[1].contains("[#1] \"Download One\" [1080p]"));
        assert!(lines[2].contains("65.0%"));
        assert!(lines[3].contains("[#2] \"Download Two\" [Audio] [Finalizing media...]"));
        assert!(lines[4].contains("Finalizing"));
    }

    #[test]
    fn test_handle_live_key() {
        assert_eq!(
            handle_live_key(KeyCode::Char('c'), false),
            LiveAction::AskCancel
        );
        assert_eq!(
            handle_live_key(KeyCode::Char('C'), false),
            LiveAction::AskCancel
        );

        assert_eq!(
            handle_live_key(KeyCode::Char('d'), false),
            LiveAction::Detach
        );
        assert_eq!(
            handle_live_key(KeyCode::Char('D'), false),
            LiveAction::Detach
        );
        assert_eq!(
            handle_live_key(KeyCode::Char('q'), false),
            LiveAction::Detach
        );
        assert_eq!(
            handle_live_key(KeyCode::Char('Q'), false),
            LiveAction::Detach
        );
        assert_eq!(handle_live_key(KeyCode::Esc, false), LiveAction::Detach);
        assert_eq!(handle_live_key(KeyCode::Enter, false), LiveAction::Detach);

        assert_eq!(handle_live_key(KeyCode::Char('x'), false), LiveAction::None);
        assert_eq!(handle_live_key(KeyCode::Char('y'), false), LiveAction::None);

        assert_eq!(
            handle_live_key(KeyCode::Char('y'), true),
            LiveAction::ConfirmCancel
        );
        assert_eq!(
            handle_live_key(KeyCode::Char('Y'), true),
            LiveAction::ConfirmCancel
        );

        assert_eq!(
            handle_live_key(KeyCode::Char('n'), true),
            LiveAction::DismissCancel
        );
        assert_eq!(
            handle_live_key(KeyCode::Char('N'), true),
            LiveAction::DismissCancel
        );
        assert_eq!(
            handle_live_key(KeyCode::Esc, true),
            LiveAction::DismissCancel
        );

        assert_eq!(
            handle_live_key(KeyCode::Char('x'), true),
            LiveAction::DismissCancel
        );
        assert_eq!(
            handle_live_key(KeyCode::Char('q'), true),
            LiveAction::DismissCancel
        );
    }
}
