use colored::Colorize;
use unicode_width::UnicodeWidthStr;

use crate::ui::theme::{ColorLevel, ThemeMode, box_chars};

/// Renders a startup banner for interactive REPL shell.
pub fn render_banner(theme: &ThemeMode, color: ColorLevel, quiet: bool) -> String {
    if quiet {
        return String::new();
    }

    let b = box_chars(theme);
    let inner_width: usize = 51;

    let dash = match theme {
        ThemeMode::Unicode => "—",
        ThemeMode::Ascii => "-",
    };
    let line1 = format!(
        "  DWDR v{} {} Terminal Media Downloader",
        env!("CARGO_PKG_VERSION"),
        dash
    );
    let line2 = "  Type a URL to start, 'help' for commands";

    let pad1 = inner_width.saturating_sub(line1.width());
    let pad2 = inner_width.saturating_sub(line2.width());

    let h_line = b.h.repeat(inner_width);
    let top = format!("{}{}{}", b.tl, h_line, b.tr);
    let mid1 = format!("{}{}{}{}", b.v, line1, " ".repeat(pad1), b.v);
    let mid2 = format!("{}{}{}{}", b.v, line2, " ".repeat(pad2), b.v);
    let bot = format!("{}{}{}", b.bl, h_line, b.br);

    let banner = format!("{}\n{}\n{}\n{}", top, mid1, mid2, bot);
    match color {
        ColorLevel::None => banner,
        _ => banner.cyan().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_banner_quiet() {
        let b = render_banner(&ThemeMode::Unicode, ColorLevel::None, true);
        assert!(b.is_empty());
    }

    #[test]
    fn test_banner_ascii() {
        let b = render_banner(&ThemeMode::Ascii, ColorLevel::None, false);
        assert!(b.contains("DWDR v"));
        assert!(b.contains("Type a URL to start"));
    }
}
