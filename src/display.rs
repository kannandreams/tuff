const CORAL_WORDMARK_ASCII: &str = include_str!("../assets/coral.txt");

const WORDMARK_GRADIENT: [(f64, (u8, u8, u8)); 5] = [
    (0.00, (255, 128, 0)),
    (0.25, (255, 105, 180)),
    (0.50, (138, 43, 226)),
    (0.75, (30, 144, 255)),
    (1.00, (0, 206, 209)),
];

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

pub fn print_welcome() {
    let use_color = std::env::var_os("NO_COLOR").is_none();
    let coral = if use_color {
        "\x1b[38;2;255;122;89m"
    } else {
        ""
    };
    let pink = if use_color {
        "\x1b[38;2;255;92;191m"
    } else {
        ""
    };
    let violet = if use_color {
        "\x1b[38;2;178;108;255m"
    } else {
        ""
    };
    let cyan = if use_color {
        "\x1b[38;2;57;208;255m"
    } else {
        ""
    };
    let mint = if use_color {
        "\x1b[38;2;63;255;196m"
    } else {
        ""
    };
    let white = if use_color {
        "\x1b[38;2;241;246;248m"
    } else {
        ""
    };
    let gray = if use_color {
        "\x1b[38;2;143;153;166m"
    } else {
        ""
    };
    let reset = if use_color { "\x1b[0m" } else { "" };

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
            hero.push_str(reset);
        }
        hero.push('\n');
    }

    let gradient_divider = format!(
        "{coral}{}{pink}{}{violet}{}{cyan}{}{mint}{}{reset}",
        "\u{2500}".repeat(18),
        "\u{2500}".repeat(18),
        "\u{2500}".repeat(18),
        "\u{2500}".repeat(18),
        "\u{2500}".repeat(18),
    );

    let title = format!(
        "{cyan}\u{2502}{reset} {mint}{:<84}{reset} {cyan}\u{2502}{reset}",
        "Quick Start"
    );
    let rows = [
        ("coral init", "Initialize .coral/coral-lock.json"),
        (
            "coral add <path> -t <target>",
            "Install a local capability to a harness",
        ),
        ("coral list", "Show installed capabilities and drift"),
        ("coral diff <id>", "Compare local artifact to baseline"),
        (
            "coral target list",
            "Show available and registered harness targets",
        ),
        ("coral --help", "Show command reference"),
    ];
    let mut quick_start = String::new();
    let border = "\u{2500}".repeat(84);
    quick_start.push_str(&format!(
        "{cyan}\u{250c}{border}\u{2510}{reset}\n"
    ));
    quick_start.push_str(&title);
    quick_start.push('\n');
    quick_start.push_str(&format!(
        "{cyan}\u{251c}{border}\u{2524}{reset}\n"
    ));
    for (index, (left, right)) in rows.iter().enumerate() {
        quick_start.push_str(&format!(
            "{cyan}\u{2502}{reset} {coral}{:<38}{reset} {cyan}\u{2502}{reset} {gray}{:<43}{reset} {cyan}\u{2502}{reset}",
            left, right
        ));
        if index + 1 < rows.len() {
            quick_start.push('\n');
        }
    }
    quick_start.push('\n');
    quick_start.push_str(&format!(
        "{cyan}\u{2514}{border}\u{2518}{reset}"
    ));

    println!(
        r#"{hero}
{gradient_divider}
{coral}Coral{reset} {white}is a capability lifecycle manager for coding agents.{reset}

{quick_start}
"#,
        hero = hero,
        gradient_divider = gradient_divider,
        quick_start = quick_start,
    );
}
