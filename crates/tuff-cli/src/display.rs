use std::io::{self, IsTerminal};

struct Palette<'a> {
    tuff: &'a str,
    white: &'a str,
    command: &'a str,
    description: &'a str,
    gray: &'a str,
    ribbon: &'a str,
    reset: &'a str,
}

fn palette(use_color: bool) -> Palette<'static> {
    if use_color {
        Palette {
            tuff: "\x1b[1;38;2;232;93;42m",
            white: "\x1b[38;2;241;246;248m",
            command: "\x1b[38;2;90;220;210m",
            description: "\x1b[38;2;190;201;207m",
            gray: "\x1b[38;2;143;153;166m",
            ribbon: "\x1b[48;2;54;56;62m",
            reset: "\x1b[0m",
        }
    } else {
        Palette {
            tuff: "",
            white: "",
            command: "",
            description: "",
            gray: "",
            ribbon: "",
            reset: "",
        }
    }
}

fn quick_start_rows() -> [(&'static str, &'static str); 6] {
    [
        ("tuff init", "Initialize Tuff state"),
        ("tuff add skill <path> [name]", "Install a capability"),
        ("tuff list", "Show installed capabilities"),
        ("tuff diff <id>", "Compare local changes to baseline"),
        ("tuff agent list", "Show configured agent harnesses"),
        ("tuff --help", "Show command reference"),
    ]
}

fn quick_start_width() -> usize {
    let rows = quick_start_rows();
    let left_width = rows
        .iter()
        .map(|(left, _)| left.chars().count())
        .max()
        .unwrap_or(0);
    let right_width = rows
        .iter()
        .map(|(_, right)| right.chars().count())
        .max()
        .unwrap_or(0);

    left_width + right_width + 7
}

fn render_ribbon(colors: &Palette<'_>, width: usize) -> String {
    let text = "Tuff is a capability lifecycle manager for coding agents.";
    let padding = width.saturating_sub(text.chars().count());

    format!(
        "{ribbon}{tuff}Tuff{white} is a capability lifecycle manager for coding agents.{padding}{reset}",
        ribbon = colors.ribbon,
        tuff = colors.tuff,
        white = colors.white,
        padding = " ".repeat(padding),
        reset = colors.reset,
    )
}

fn render_quick_start(colors: &Palette<'_>) -> String {
    let rows = quick_start_rows();

    let left_width = rows
        .iter()
        .map(|(left, _)| left.chars().count())
        .max()
        .unwrap_or(0);
    let right_width = rows
        .iter()
        .map(|(_, right)| right.chars().count())
        .max()
        .unwrap_or(0);
    let inner_width = left_width + right_width + 5;
    let title_width = inner_width.saturating_sub(2);
    let title = "Quick Start";
    let border = "\u{2500}".repeat(inner_width);

    let mut out = String::new();
    out.push_str(&format!(
        "{gray}+{border}+{reset}\n",
        gray = colors.gray,
        reset = colors.reset
    ));
    out.push_str(&format!(
        "{gray}|{reset} {white}{title:<title_width$}{reset} {gray}|{reset}\n",
        gray = colors.gray,
        white = colors.white,
        reset = colors.reset,
        title = title,
        title_width = title_width
    ));
    out.push_str(&format!(
        "{gray}+{border}+{reset}\n",
        gray = colors.gray,
        reset = colors.reset
    ));

    for (index, (left, right)) in rows.iter().enumerate() {
        out.push_str(&format!(
            "{gray}|{reset} {command}{left:<left_width$}{reset} {gray}|{reset} {description}{right:<right_width$}{reset} {gray}|{reset}",
            gray = colors.gray,
            command = colors.command,
            description = colors.description,
            reset = colors.reset,
            left = left,
            right = right,
            left_width = left_width,
            right_width = right_width
        ));
        if index + 1 < rows.len() {
            out.push('\n');
        }
    }

    out.push('\n');
    out.push_str(&format!(
        "{gray}+{border}+{reset}",
        gray = colors.gray,
        reset = colors.reset
    ));
    out
}

fn render_welcome(use_color: bool) -> String {
    let colors = palette(use_color);
    let quick_start = render_quick_start(&colors);
    let ribbon = render_ribbon(&colors, quick_start_width());

    format!(
        "{ribbon}\n\n{quick_start}\n",
        ribbon = ribbon,
        quick_start = quick_start
    )
}

fn render_init_banner(use_color: bool) -> String {
    let colors = palette(use_color);
    let ribbon = render_ribbon(&colors, quick_start_width());

    format!("{ribbon}\n", ribbon = ribbon)
}

pub fn print_welcome() {
    let use_color = use_color();
    print!("\n{}", render_welcome(use_color));
}

pub fn print_init_banner() {
    let use_color = use_color();
    print!("\n{}", render_init_banner(use_color));
}

pub(crate) fn use_color() -> bool {
    color_enabled(
        io::stdout().is_terminal(),
        std::env::var_os("NO_COLOR").is_some(),
    )
}

fn color_enabled(is_terminal: bool, no_color_is_set: bool) -> bool {
    is_terminal && !no_color_is_set
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_start_box_closes_each_row_without_color() {
        let text = render_quick_start(&palette(false));
        let lines: Vec<&str> = text.lines().collect();
        assert!(!lines.is_empty());
        assert!(lines[0].starts_with('+'));
        assert!(lines[0].ends_with('+'));
        assert!(lines[1].starts_with('|'));
        assert!(lines[1].ends_with('|'));
        assert!(lines[2].starts_with('+'));
        assert!(lines[2].ends_with('+'));
        for line in &lines[3..lines.len() - 1] {
            assert!(line.starts_with('|'));
            assert!(line.ends_with('|'));
        }
        assert!(lines.last().unwrap().starts_with('+'));
        assert!(lines.last().unwrap().ends_with('+'));
    }

    #[test]
    fn init_banner_is_more_compact_than_full_welcome() {
        let welcome = render_welcome(false);
        let init = render_init_banner(false);
        assert!(welcome.contains("Quick Start"));
        assert!(!init.contains("Quick Start"));
        assert!(init.contains("capability lifecycle manager"));
    }

    #[test]
    fn tagline_is_first_banner_content() {
        let welcome = render_welcome(false);
        assert!(welcome.contains("Tuff is a capability lifecycle manager for coding agents."));
        assert!(!welcome.contains("▄▄"));
        assert!(!welcome.contains("████"));
    }

    #[test]
    fn ribbon_matches_quick_start_box_width_without_color() {
        let ribbon = render_ribbon(&palette(false), quick_start_width());
        let quick_start = render_quick_start(&palette(false));
        let box_line = quick_start
            .lines()
            .next()
            .expect("quick-start box has a top border");

        assert_eq!(ribbon.chars().count(), box_line.chars().count());
    }

    #[test]
    fn color_requires_a_terminal_and_respects_no_color() {
        assert!(color_enabled(true, false));
        assert!(!color_enabled(false, false));
        assert!(!color_enabled(true, true));
        assert!(!color_enabled(false, true));
    }
}
