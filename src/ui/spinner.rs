use std::io::IsTerminal;
use std::time::Instant;

use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};

use crate::ui::theme::{
    ColorLevel, SPINNER_ASCII, SPINNER_UNICODE, ThemeMode, symbol_check, symbol_cross,
};

/// Braille / ASCII spinner wrapper for asynchronous background tasks.
pub struct Spinner {
    pub start_time: Instant,
    pub message: String,
    pub pb: Option<ProgressBar>,
    pub theme: ThemeMode,
    pub color: ColorLevel,
}

impl Spinner {
    pub fn new(message: &str, theme: ThemeMode, color: ColorLevel, animations: bool) -> Self {
        let is_tty = std::io::stdout().is_terminal();
        let pb = if animations && is_tty && color != ColorLevel::None {
            let pb = ProgressBar::new_spinner();
            let tick_strings = match theme {
                ThemeMode::Unicode => SPINNER_UNICODE,
                ThemeMode::Ascii => SPINNER_ASCII,
            };
            let template = match color {
                ColorLevel::None => "{spinner} {msg}",
                _ => "{spinner:.cyan} {msg}",
            };
            if let Ok(style) = ProgressStyle::default_spinner()
                .tick_strings(tick_strings)
                .template(template)
            {
                pb.set_style(style);
            }
            pb.set_message(message.to_string());
            pb.enable_steady_tick(std::time::Duration::from_millis(80));
            Some(pb)
        } else {
            match color {
                ColorLevel::None => println!("INFO: {}", message),
                _ => println!("{}", format!("INFO: {}", message).cyan()),
            }
            None
        };

        Self {
            start_time: Instant::now(),
            message: message.to_string(),
            pb,
            theme,
            color,
        }
    }

    /// Completes the spinner and prints a single-line success notification with duration.
    pub fn finish_success(&self, result_msg: &str) {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if let Some(pb) = &self.pb {
            pb.finish_and_clear();
        }
        let check = symbol_check(&self.theme);
        match self.color {
            ColorLevel::None => {
                println!("SUCCESS: {} ({} {:.1}s)", result_msg, check, elapsed);
            }
            _ => {
                println!(
                    "{}",
                    format!("SUCCESS: {} ({} {:.1}s)", result_msg, check, elapsed).green()
                );
            }
        }
    }

    /// Completes the spinner and prints an informational notification with duration.
    pub fn finish_info(&self, result_msg: &str) {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if let Some(pb) = &self.pb {
            pb.finish_and_clear();
        }
        let check = symbol_check(&self.theme);
        match self.color {
            ColorLevel::None => {
                println!("INFO: {} ({} {:.1}s)", result_msg, check, elapsed);
            }
            _ => {
                println!(
                    "{}",
                    format!("INFO: {} ({} {:.1}s)", result_msg, check, elapsed).cyan()
                );
            }
        }
    }

    /// Completes the spinner and prints an error notification.
    pub fn finish_error(&self, err_msg: &str) {
        if let Some(pb) = &self.pb {
            pb.finish_and_clear();
        }
        let cross = symbol_cross(&self.theme);
        match self.color {
            ColorLevel::None => {
                eprintln!("ERROR: {} ({})", err_msg, cross);
            }
            _ => {
                eprintln!("{}", format!("ERROR: {} ({})", err_msg, cross).red().bold());
            }
        }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        if let Some(pb) = &self.pb
            && !pb.is_finished()
        {
            pb.finish_and_clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spinner_plain() {
        let s = Spinner::new(
            "Loading test data",
            ThemeMode::Ascii,
            ColorLevel::None,
            false,
        );
        assert!(s.pb.is_none());
        s.finish_success("Done");
    }

    #[test]
    fn test_spinner_error_and_drop() {
        let s = Spinner::new(
            "Loading fail data",
            ThemeMode::Ascii,
            ColorLevel::None,
            false,
        );
        s.finish_error("Failed");

        // Test drop without explicit finish
        let _s2 = Spinner::new("Early drop test", ThemeMode::Ascii, ColorLevel::None, false);
    }
}
