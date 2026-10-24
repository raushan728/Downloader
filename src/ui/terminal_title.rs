use std::io::{IsTerminal, Write, stdout};
use std::sync::Mutex;

use crate::ui::UiContext;
use crate::ui::text::sanitize_display;

static LAST_TITLE: Mutex<Option<String>> = Mutex::new(None);
static LAST_TASKBAR_PERCENT: Mutex<Option<u8>> = Mutex::new(None);

fn is_taskbar_progress_supported() -> bool {
    std::env::var("WT_SESSION").is_ok()
        || std::env::var("TERM_PROGRAM").ok().as_deref() == Some("iTerm.app")
        || std::env::var("ConEmuPID").is_ok()
}

/// Sets the terminal window/tab title using OSC 0 (`\x1b]0;{title}\x07`).
///
/// Preconditions:
/// - Suppressed when stdout is not a TTY or when animations are disabled in `UiContext`.
/// - Sanitizes the title against escape injection.
/// - Deduplicates identical consecutive writes to prevent redundant I/O.
pub fn set_terminal_title(title: &str) {
    let ctx = UiContext::get();
    if !ctx.is_tty || !ctx.animations {
        return;
    }
    let clean = sanitize_display(title);
    let mut last = match LAST_TITLE.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    if last.as_deref() == Some(&clean) {
        return;
    }
    *last = Some(clean.clone());
    print!("\x1b]0;{}\x07", clean);
    let _ = stdout().flush();
}

/// Updates OS taskbar progress indicator using OSC 9;4 for supported terminals (Windows Terminal, iTerm, ConEmu).
pub fn set_taskbar_progress(percent: u8) {
    let p = percent.min(100);
    let mut last = match LAST_TASKBAR_PERCENT.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    if *last == Some(p) {
        return;
    }
    *last = Some(p);
    if stdout().is_terminal() && is_taskbar_progress_supported() {
        print!("\x1b]9;4;1;{}\x07", p);
        let _ = stdout().flush();
    }
}

/// Clears OS taskbar progress indicator.
pub fn clear_taskbar_progress() {
    let mut last = match LAST_TASKBAR_PERCENT.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    if last.is_none() {
        return;
    }
    *last = None;
    if stdout().is_terminal() && is_taskbar_progress_supported() {
        print!("\x1b]9;4;0;0\x07");
        let _ = stdout().flush();
    }
}

/// Rings the terminal bell if configured.
pub fn terminal_bell() {
    if stdout().is_terminal() {
        print!("\x07");
        let _ = stdout().flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_taskbar_progress_guard() {
        // Safe to call even in headless / test environment without error, and deduplicates identical calls
        set_taskbar_progress(50);
        set_taskbar_progress(50);
        assert_eq!(*LAST_TASKBAR_PERCENT.lock().unwrap(), Some(50));
        clear_taskbar_progress();
        assert_eq!(*LAST_TASKBAR_PERCENT.lock().unwrap(), None);
    }

    #[test]
    fn test_terminal_title_safe_invocation() {
        set_terminal_title("DWDR - \u{001b}[2JTest");
        set_terminal_title("DWDR - \u{001b}[2JTest"); // Deduplicated
    }
}
