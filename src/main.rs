use std::{
    collections::BTreeMap,
    error::Error,
    fmt::{self, Display},
    fs,
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use similar::{ChangeTag, TextDiff};

const LOCKFILE_VERSION: u8 = 1;
const SUPPORTED_KIND: &str = "skill";
const SUPPORTED_TARGET: &str = "codex";
const SUPPORTED_FILES: [&str; 1] = ["src/SKILL.md"];
const CORAL_WORDMARK_ASCII: &str = include_str!("../assets/coral.txt");

/// Gradient stops (position, RGB) used to color the wordmark.
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

type Result<T> = std::result::Result<T, CoralError>;

#[derive(Debug)]
struct CoralError(String);

impl CoralError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl Display for CoralError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for CoralError {}

impl From<std::io::Error> for CoralError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<serde_json::Error> for CoralError {
    fn from(error: serde_json::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<toml::de::Error> for CoralError {
    fn from(error: toml::de::Error) -> Self {
        Self(format!("invalid capability manifest TOML: {error}"))
    }
}

#[derive(Parser)]
#[command(name = "coral", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize .coral state.
    Init,
    /// Install a local capability.
    Add {
        /// Path to a capability directory.
        capability: PathBuf,
    },
    /// List installed capabilities.
    List,
    /// Diff an installed capability against baseline.
    Diff {
        /// Installed capability id.
        capability_id: String,
    },
}

#[derive(Debug, Deserialize)]
struct PrimitiveManifest {
    id: String,
    version: String,
    kind: String,
    target: String,
    description: String,
    files: Vec<String>,

    #[serde(skip)]
    root: PathBuf,
}

impl PrimitiveManifest {
    fn skill_source_path(&self) -> Result<PathBuf> {
        if self.files != SUPPORTED_FILES {
            return Err(CoralError::new(
                "codex skill capabilities must declare files = ['src/SKILL.md']",
            ));
        }
        Ok(self.root.join("src").join("SKILL.md"))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Lockfile {
    version: u8,
    primitives: BTreeMap<String, PrimitiveLockEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PrimitiveLockEntry {
    version: String,
    source_path: String,
    installed_target_path: String,
    baseline_path: String,
    baseline_content_hash: String,
    installed_content_hash: String,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let repo_root = std::env::current_dir()?;

    match cli.command {
        None => {
            print_welcome();
        }
        Some(Command::Init) => {
            let lock_path = init_lockfile(&repo_root)?;
            println!("initialized {}", relative_display(&lock_path, &repo_root));
        }
        Some(Command::Add { capability }) => {
            let mut lockfile = require_lockfile(&repo_root)?;
            let capability_dir = absolutize(&repo_root, &capability);
            let manifest = load_manifest(&capability_dir)?;
            let target_path = install_codex_skill(&repo_root, &mut lockfile, &manifest)?;
            write_lockfile(&repo_root, &lockfile)?;
            println!(
                "installed {} -> {}",
                manifest.id,
                relative_display(&target_path, &repo_root)
            );
        }
        Some(Command::List) => {
            let lockfile = require_lockfile(&repo_root)?;
            if lockfile.primitives.is_empty() {
                println!("no primitives installed");
                return Ok(());
            }

            for (primitive_id, entry) in lockfile.primitives {
                let status = drift_status(&repo_root, &entry);
                println!(
                    "{}\t{}\t{}\t{}",
                    primitive_id, entry.version, status, entry.installed_target_path
                );
            }
        }
        Some(Command::Diff { capability_id }) => {
            let lockfile = require_lockfile(&repo_root)?;
            let entry = lockfile.primitives.get(&capability_id).ok_or_else(|| {
                CoralError::new(format!("capability is not installed: {capability_id}"))
            })?;
            print!(
                "{}",
                diff_against_baseline(&repo_root, &capability_id, entry)?
            );
        }
    }

    Ok(())
}

fn print_welcome() {
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
        "─".repeat(18),
        "─".repeat(18),
        "─".repeat(18),
        "─".repeat(18),
        "─".repeat(18),
    );

    let title = format!(
        "{cyan}│{reset} {mint}{:<84}{reset} {cyan}│{reset}",
        "Quick Start"
    );
    let rows = [
        ("coral init", "Initialize .coral/lock.json"),
        ("coral add <path>", "Install a local capability"),
        ("coral list", "Show installed capabilities and drift"),
        ("coral diff <id>", "Compare local artifact to baseline"),
        ("coral --help", "Show command reference"),
    ];
    let mut quick_start = String::new();
    quick_start.push_str(&format!(
        "{cyan}┌──────────────────────────────────────────────────────────────────────────────────────┐{reset}\n"
    ));
    quick_start.push_str(&title);
    quick_start.push('\n');
    quick_start.push_str(&format!(
        "{cyan}├─────────────────────────┬────────────────────────────────────────────────────────────┤{reset}\n"
    ));
    for (index, (left, right)) in rows.iter().enumerate() {
        quick_start.push_str(&format!(
            "{cyan}│{reset} {coral}{:<23}{reset} {cyan}│{reset} {gray}{:<58}{reset} {cyan}│{reset}",
            left, right
        ));
        if index + 1 < rows.len() {
            quick_start.push('\n');
        }
    }
    quick_start.push('\n');
    quick_start.push_str(&format!(
        "{cyan}└─────────────────────────┴────────────────────────────────────────────────────────────┘{reset}"
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

fn init_lockfile(repo_root: &Path) -> Result<PathBuf> {
    let coral_dir = repo_root.join(".coral");
    fs::create_dir_all(&coral_dir)?;
    let lock_path = coral_dir.join("lock.json");
    if !lock_path.exists() {
        write_lockfile(
            repo_root,
            &Lockfile {
                version: LOCKFILE_VERSION,
                primitives: BTreeMap::new(),
            },
        )?;
    }
    Ok(lock_path)
}

fn require_lockfile(repo_root: &Path) -> Result<Lockfile> {
    let lock_path = repo_root.join(".coral").join("lock.json");
    if !lock_path.exists() {
        return Err(CoralError::new(
            ".coral/lock.json is missing; run 'coral init' first",
        ));
    }

    let lockfile: Lockfile = serde_json::from_str(&fs::read_to_string(lock_path)?)?;
    if lockfile.version != LOCKFILE_VERSION {
        return Err(CoralError::new(format!(
            "unsupported lockfile version: {}",
            lockfile.version
        )));
    }
    Ok(lockfile)
}

fn write_lockfile(repo_root: &Path, lockfile: &Lockfile) -> Result<()> {
    let lock_path = repo_root.join(".coral").join("lock.json");
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(lock_path, serde_json::to_string_pretty(lockfile)? + "\n")?;
    Ok(())
}

fn load_manifest(primitive_dir: &Path) -> Result<PrimitiveManifest> {
    let manifest_path = primitive_dir.join("coral.toml");
    if !manifest_path.exists() {
        return Err(CoralError::new(format!(
            "capability manifest not found: {}",
            manifest_path.display()
        )));
    }

    let mut manifest: PrimitiveManifest = toml::from_str(&fs::read_to_string(&manifest_path)?)?;
    manifest.root = primitive_dir.to_path_buf();

    validate_non_empty("id", &manifest.id)?;
    validate_non_empty("version", &manifest.version)?;
    validate_non_empty("kind", &manifest.kind)?;
    validate_non_empty("target", &manifest.target)?;
    validate_non_empty("description", &manifest.description)?;

    if manifest.kind != SUPPORTED_KIND {
        return Err(CoralError::new(format!(
            "unsupported capability kind '{}'; only '{}' is supported",
            manifest.kind, SUPPORTED_KIND
        )));
    }
    if manifest.target != SUPPORTED_TARGET {
        return Err(CoralError::new(format!(
            "unsupported capability target '{}'; only '{}' is supported",
            manifest.target, SUPPORTED_TARGET
        )));
    }

    let source_path = manifest.skill_source_path()?;
    if !source_path.exists() {
        return Err(CoralError::new(format!(
            "capability source file not found: {}",
            source_path.display()
        )));
    }

    Ok(manifest)
}

fn validate_non_empty(field: &str, value: &str) -> Result<()> {
    if value.is_empty() {
        return Err(CoralError::new(format!(
            "capability manifest field '{field}' must be a non-empty string"
        )));
    }
    Ok(())
}

fn install_codex_skill(
    repo_root: &Path,
    lockfile: &mut Lockfile,
    manifest: &PrimitiveManifest,
) -> Result<PathBuf> {
    let source_path = manifest.skill_source_path()?;
    let target_path = repo_root
        .join(".agents")
        .join("skills")
        .join(&manifest.id)
        .join("SKILL.md");
    let baseline_path = repo_root
        .join(".coral")
        .join("baselines")
        .join(&manifest.id)
        .join("SKILL.md");

    if target_path.exists() && !lockfile.primitives.contains_key(&manifest.id) {
        return Err(CoralError::new(format!(
            "refusing to overwrite untracked skill at {}; remove it or track it in Coral first",
            target_path.display()
        )));
    }

    let content = fs::read(&source_path)?;
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(parent) = baseline_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&target_path, &content)?;
    fs::write(&baseline_path, &content)?;

    let content_hash = hash_bytes(&content);
    lockfile.primitives.insert(
        manifest.id.clone(),
        PrimitiveLockEntry {
            version: manifest.version.clone(),
            source_path: relative_or_absolute(&source_path, repo_root),
            installed_target_path: relative_or_absolute(&target_path, repo_root),
            baseline_path: relative_or_absolute(&baseline_path, repo_root),
            baseline_content_hash: content_hash.clone(),
            installed_content_hash: content_hash,
        },
    );

    Ok(target_path)
}

fn drift_status(repo_root: &Path, entry: &PrimitiveLockEntry) -> &'static str {
    let target_path = repo_root.join(&entry.installed_target_path);
    if !target_path.exists() {
        return "missing";
    }

    match fs::read(&target_path) {
        Ok(content) if hash_bytes(&content) == entry.installed_content_hash => "clean",
        Ok(_) => "modified",
        Err(_) => "missing",
    }
}

fn diff_against_baseline(
    repo_root: &Path,
    primitive_id: &str,
    entry: &PrimitiveLockEntry,
) -> Result<String> {
    let baseline_path = repo_root.join(&entry.baseline_path);
    let target_path = repo_root.join(&entry.installed_target_path);
    if !baseline_path.exists() {
        return Err(CoralError::new(format!(
            "baseline file missing for '{}': {}",
            primitive_id,
            baseline_path.display()
        )));
    }
    if !target_path.exists() {
        return Err(CoralError::new(format!(
            "installed file missing for '{}': {}",
            primitive_id,
            target_path.display()
        )));
    }

    let baseline = fs::read_to_string(&baseline_path)?;
    let target = fs::read_to_string(&target_path)?;
    if baseline == target {
        return Ok(String::new());
    }

    let diff = TextDiff::from_lines(&baseline, &target);
    let mut output = format!(
        "--- baseline/{primitive_id}/SKILL.md\n+++ {}\n",
        entry.installed_target_path
    );
    for group in diff.grouped_ops(3) {
        for operation in group {
            for change in diff.iter_changes(&operation) {
                let sign = match change.tag() {
                    ChangeTag::Delete => "-",
                    ChangeTag::Insert => "+",
                    ChangeTag::Equal => " ",
                };
                output.push_str(sign);
                output.push_str(change.value());
            }
        }
    }
    Ok(output)
}

fn hash_bytes(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    format!("{:x}", hasher.finalize())
}

fn absolutize(repo_root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo_root.join(path)
    }
}

fn relative_display(path: &Path, repo_root: &Path) -> String {
    relative_or_absolute(path, repo_root)
}

fn relative_or_absolute(path: &Path, repo_root: &Path) -> String {
    path.strip_prefix(repo_root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().to_string())
}
