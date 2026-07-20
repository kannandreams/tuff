const CORAL_WORDMARK_ASCII: &str = include_str!("../assets/coral-alien-block.txt");

const WORDMARK_GRADIENT: [(f64, (u8, u8, u8)); 5] = [
    (0.00, (255, 128, 0)),
    (0.25, (255, 105, 180)),
    (0.50, (138, 43, 226)),
    (0.75, (30, 144, 255)),
    (1.00, (0, 206, 209)),
];

struct Palette<'a> {
    coral: &'a str,
    pink: &'a str,
    violet: &'a str,
    cyan: &'a str,
    mint: &'a str,
    white: &'a str,
    gray: &'a str,
    reset: &'a str,
}

fn palette(use_color: bool) -> Palette<'static> {
    if use_color {
        Palette {
            coral: "\x1b[38;2;255;122;89m",
            pink: "\x1b[38;2;255;92;191m",
            violet: "\x1b[38;2;178;108;255m",
            cyan: "\x1b[38;2;57;208;255m",
            mint: "\x1b[38;2;63;255;196m",
            white: "\x1b[38;2;241;246;248m",
            gray: "\x1b[38;2;143;153;166m",
            reset: "\x1b[0m",
        }
    } else {
        Palette {
            coral: "",
            pink: "",
            violet: "",
            cyan: "",
            mint: "",
            white: "",
            gray: "",
            reset: "",
        }
    }
}

fn lerp_u8(a: u8, b: u8, t: f64) -> u8 {
    (a as f64 + (b as f64 - a as f64) * t).round() as u8
}

fn wordmark_color_at(t: f64) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    for window in WORDMARK_GRADIENT.windows(2) {
        let (t0, c0) = window[0];
        let (t1, c1) = window[1];
        if t <= t1 {
            let frac = if t1 != t0 { (t - t0) / (t1 - t0) } else { 0.0 };
            return (
                lerp_u8(c0.0, c1.0, frac),
                lerp_u8(c0.1, c1.1, frac),
                lerp_u8(c0.2, c1.2, frac),
            );
        }
    }
    WORDMARK_GRADIENT[WORDMARK_GRADIENT.len() - 1].1
}

fn render_wordmark(use_color: bool) -> String {
    let logo_lines = CORAL_WORDMARK_ASCII
        .lines()
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let logo_width = logo_lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);

    let mut hero = String::new();
    for line in &logo_lines {
        for (char_index, ch) in line.chars().enumerate() {
            if ch == ' ' {
                hero.push(' ');
                continue;
            }

            let tx = if logo_width <= 1 {
                0.0
            } else {
                char_index as f64 / (logo_width - 1) as f64
            };
            if use_color {
                let (r, g, b) = wordmark_color_at(tx);
                hero.push_str(&format!("\x1b[38;2;{r};{g};{b}m"));
            }
            hero.push(ch);
        }
        if use_color {
            hero.push_str("\x1b[0m");
        }
        hero.push('\n');
    }

    hero
}

fn render_divider(colors: &Palette<'_>) -> String {
    format!(
        "{c0}{}{c1}{}{c2}{}{c3}{}{c4}{}{reset}",
        "\u{2500}".repeat(18),
        "\u{2500}".repeat(18),
        "\u{2500}".repeat(18),
        "\u{2500}".repeat(18),
        "\u{2500}".repeat(18),
        c0 = colors.coral,
        c1 = colors.pink,
        c2 = colors.violet,
        c3 = colors.cyan,
        c4 = colors.mint,
        reset = colors.reset,
    )
}

fn render_quick_start(colors: &Palette<'_>) -> String {
    let rows = [
        ("coral init", "Initialize Coral state"),
        ("coral add skill <path> [name]", "Install a capability"),
        ("coral list", "Show installed capabilities"),
        ("coral diff <id>", "Compare local changes to baseline"),
        ("coral agent list", "Show configured agent harnesses"),
        ("coral --help", "Show command reference"),
    ];

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
        "{cyan}+{border}+{reset}\n",
        cyan = colors.cyan,
        reset = colors.reset
    ));
    out.push_str(&format!(
        "{cyan}|{reset} {mint}{title:<title_width$}{reset} {cyan}|{reset}\n",
        cyan = colors.cyan,
        mint = colors.mint,
        reset = colors.reset,
        title = title,
        title_width = title_width
    ));
    out.push_str(&format!(
        "{cyan}+{border}+{reset}\n",
        cyan = colors.cyan,
        reset = colors.reset
    ));

    for (index, (left, right)) in rows.iter().enumerate() {
        out.push_str(&format!(
            "{cyan}|{reset} {coral}{left:<left_width$}{reset} {cyan}|{reset} {gray}{right:<right_width$}{reset} {cyan}|{reset}",
            cyan = colors.cyan,
            coral = colors.coral,
            gray = colors.gray,
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
        "{cyan}+{border}+{reset}",
        cyan = colors.cyan,
        reset = colors.reset
    ));
    out
}

fn render_tagline(colors: &Palette<'_>) -> String {
    format!(
        "{coral}Coral{reset} {white}is a capability lifecycle manager for coding agents.{reset}",
        coral = colors.coral,
        white = colors.white,
        reset = colors.reset
    )
}

fn render_welcome(use_color: bool) -> String {
    let colors = palette(use_color);
    let hero = render_wordmark(use_color);
    let divider = render_divider(&colors);
    let quick_start = render_quick_start(&colors);
    let tagline = render_tagline(&colors);

    format!(
        "{hero}\n{divider}\n{tagline}\n\n{quick_start}\n",
        hero = hero,
        divider = divider,
        tagline = tagline,
        quick_start = quick_start
    )
}

fn render_init_banner(use_color: bool) -> String {
    let colors = palette(use_color);
    let hero = render_wordmark(use_color);
    let divider = render_divider(&colors);
    let tagline = render_tagline(&colors);

    format!(
        "{hero}\n{divider}\n{tagline}\n",
        hero = hero,
        divider = divider,
        tagline = tagline
    )
}

pub fn print_welcome() {
    let use_color = std::env::var_os("NO_COLOR").is_none();
    print!("{}", render_welcome(use_color));
}

pub fn print_init_banner() {
    let use_color = std::env::var_os("NO_COLOR").is_none();
    print!("{}", render_init_banner(use_color));
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
    fn wordmark_uses_alien_block_asset() {
        let wordmark = render_wordmark(false);
        assert!(wordmark.contains("▄▄"));
        assert!(!wordmark.contains("██████   ██████  ██████"));
    }
}
