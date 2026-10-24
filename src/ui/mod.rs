pub mod banner;
pub mod card;
pub mod format_table;
pub mod progress;
pub mod smooth;
pub mod spinner;
pub mod terminal_title;
pub mod text;
pub mod theme;

use std::io::{IsTerminal, stdout};
use std::sync::RwLock;

use crate::config::manager::DwdrConfig;
use crate::ui::theme::{ColorLevel, ThemeMode};

/// Central runtime context managing display mode, colors, animations, and terminal geometry.
#[derive(Debug, Clone)]
pub struct UiContext {
    pub theme: ThemeMode,
    pub color_level: ColorLevel,
    pub animations: bool,
    pub is_tty: bool,
    pub term_width: usize,
    pub quiet: bool,
    pub no_animation: bool,
    pub auto_watch: bool,
    pub bell: bool,
}

static GLOBAL_UI_CONTEXT: RwLock<Option<UiContext>> = RwLock::new(None);

impl UiContext {
    /// Reads the live terminal width without caching across the session.
    pub fn current_width() -> usize {
        if stdout().is_terminal() {
            crossterm::terminal::size()
                .map(|(w, _)| w as usize)
                .unwrap_or(80)
        } else {
            80
        }
    }

    pub fn init(config: &DwdrConfig, no_animation: bool, quiet: bool) -> UiContext {
        let is_tty = stdout().is_terminal();
        let is_no_color = std::env::var("NO_COLOR").is_ok_and(|v| !v.is_empty());
        let color_level = if !is_tty || is_no_color {
            ColorLevel::None
        } else {
            ColorLevel::detect()
        };

        if color_level == ColorLevel::None {
            colored::control::set_override(false);
        } else {
            colored::control::unset_override();
        }

        let animations =
            is_tty && config.animations && !no_animation && color_level != ColorLevel::None;

        let ctx = UiContext {
            theme: ThemeMode::detect(&config.theme),
            color_level,
            animations,
            is_tty,
            term_width: Self::current_width(),
            quiet,
            no_animation,
            auto_watch: config.auto_watch,
            bell: config.bell,
        };

        let mut guard = match GLOBAL_UI_CONTEXT.write() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        *guard = Some(ctx.clone());
        ctx
    }

    /// Refreshes the active UI context when configuration changes.
    pub fn refresh(config: &DwdrConfig) {
        let mut guard = match GLOBAL_UI_CONTEXT.write() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let is_tty = stdout().is_terminal();
        let is_no_color = std::env::var("NO_COLOR").is_ok_and(|v| !v.is_empty());
        let color_level = if !is_tty || is_no_color {
            ColorLevel::None
        } else {
            ColorLevel::detect()
        };

        if color_level == ColorLevel::None {
            colored::control::set_override(false);
        } else {
            colored::control::unset_override();
        }

        let prev_quiet = guard.as_ref().map(|c| c.quiet).unwrap_or(false);
        let prev_no_animation = guard.as_ref().map(|c| c.no_animation).unwrap_or(false);
        let animations =
            is_tty && config.animations && !prev_no_animation && color_level != ColorLevel::None;

        *guard = Some(UiContext {
            theme: ThemeMode::detect(&config.theme),
            color_level,
            animations,
            is_tty,
            term_width: Self::current_width(),
            quiet: prev_quiet,
            no_animation: prev_no_animation,
            auto_watch: config.auto_watch,
            bell: config.bell,
        });
    }

    pub fn get() -> UiContext {
        let guard = match GLOBAL_UI_CONTEXT.read() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(ctx) = guard.as_ref() {
            let mut cloned = ctx.clone();
            cloned.term_width = Self::current_width();
            return cloned;
        }
        drop(guard);
        let default_config = DwdrConfig::default();
        Self::init(&default_config, false, false)
    }

    pub fn is_plain(&self) -> bool {
        !self.is_tty || self.color_level == ColorLevel::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_ui_context_plain_mode() {
        let plain_ctx = UiContext {
            theme: ThemeMode::Ascii,
            color_level: ColorLevel::None,
            animations: false,
            is_tty: true,
            term_width: 80,
            quiet: false,
            no_animation: false,
            auto_watch: true,
            bell: false,
        };
        assert!(plain_ctx.is_plain());

        let non_tty = UiContext {
            theme: ThemeMode::Ascii,
            color_level: ColorLevel::TrueColor,
            animations: false,
            is_tty: false,
            term_width: 80,
            quiet: false,
            no_animation: false,
            auto_watch: true,
            bell: false,
        };
        assert!(non_tty.is_plain());

        let rich = UiContext {
            theme: ThemeMode::Unicode,
            color_level: ColorLevel::TrueColor,
            animations: true,
            is_tty: true,
            term_width: 80,
            quiet: false,
            no_animation: false,
            auto_watch: true,
            bell: false,
        };
        assert!(!rich.is_plain());
    }

    #[serial]
    #[test]
    fn test_ui_context_refresh() {
        let mut cfg = DwdrConfig {
            theme: "ascii".to_string(),
            animations: false,
            ..Default::default()
        };
        UiContext::init(&cfg, true, true);

        let ctx = UiContext::get();
        assert_eq!(ctx.theme, ThemeMode::Ascii);
        assert!(!ctx.animations);
        assert!(ctx.quiet);
        assert!(ctx.no_animation);

        cfg.theme = "unicode".to_string();
        cfg.animations = true;
        UiContext::refresh(&cfg);

        let updated = UiContext::get();
        assert_eq!(updated.theme, ThemeMode::Unicode);
        // Quiet and no_animation survive refresh; animations stays false because no_animation is true
        assert!(updated.quiet);
        assert!(updated.no_animation);
        assert!(!updated.animations);
    }

    #[serial]
    #[test]
    fn test_ui_context_current_width() {
        let w = UiContext::current_width();
        assert!(w > 0);
    }
}
