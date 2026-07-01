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
const LOADOUT_ASCII: &str = include_str!("../assets/loadout.txt");

type Result<T> = std::result::Result<T, LoadoutError>;

#[derive(Debug)]
struct LoadoutError(String);

impl LoadoutError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl Display for LoadoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for LoadoutError {}

impl From<std::io::Error> for LoadoutError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<serde_json::Error> for LoadoutError {
    fn from(error: serde_json::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<toml::de::Error> for LoadoutError {
    fn from(error: toml::de::Error) -> Self {
        Self(format!("invalid primitive manifest TOML: {error}"))
    }
}

#[derive(Parser)]
#[command(name = "loadout", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize .loadout state.
    Init,
    /// Install a local primitive.
    Add {
        /// Path to a primitive directory.
        primitive: PathBuf,
    },
    /// List installed primitives.
    List,
    /// Diff an installed primitive against baseline.
    Diff {
        /// Installed primitive id.
        primitive_id: String,
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
            return Err(LoadoutError::new(
                "codex skill primitives must declare files = ['src/SKILL.md']",
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
        Some(Command::Add { primitive }) => {
            let mut lockfile = require_lockfile(&repo_root)?;
            let primitive_dir = absolutize(&repo_root, &primitive);
            let manifest = load_manifest(&primitive_dir)?;
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
        Some(Command::Diff { primitive_id }) => {
            let lockfile = require_lockfile(&repo_root)?;
            let entry = lockfile.primitives.get(&primitive_id).ok_or_else(|| {
                LoadoutError::new(format!("primitive is not installed: {primitive_id}"))
            })?;
            print!(
                "{}",
                diff_against_baseline(&repo_root, &primitive_id, entry)?
            );
        }
    }

    Ok(())
}

fn print_welcome() {
    let use_color = std::env::var_os("NO_COLOR").is_none();
    let army_green = if use_color { "\x1b[38;5;64m" } else { "" };
    let gray = if use_color { "\x1b[38;5;245m" } else { "" };
    let white = if use_color { "\x1b[38;5;250m" } else { "" };
    let bold = if use_color { "\x1b[1m" } else { "" };
    let reset = if use_color { "\x1b[0m" } else { "" };

    println!(
        r#"{bold}{army_green}{LOADOUT_ASCII}{reset}

{gray}cross-harness primitives manager for coding agents{reset}
{gray}v{version}{reset}

{white}Loadout{reset} manages repo-local agent primitives.
{gray}Install, track, diff, and eventually merge skills/tools/hooks/workflows.{reset}

{white}Start here:{reset}
  {army_green}loadout init{reset}                         Create .loadout/lock.json
  {army_green}loadout add <primitive-path>{reset}         Install a local primitive
  {army_green}loadout list{reset}                         Show installed primitives and drift
  {army_green}loadout diff <id>{reset}                    Compare local artifact to baseline

{white}More:{reset}
  {army_green}loadout --help{reset}                       Show command reference
  {army_green}loadout --version{reset}                    Show installed version
"#,
        version = env!("CARGO_PKG_VERSION")
    );
}

fn init_lockfile(repo_root: &Path) -> Result<PathBuf> {
    let loadout_dir = repo_root.join(".loadout");
    fs::create_dir_all(&loadout_dir)?;
    let lock_path = loadout_dir.join("lock.json");
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
    let lock_path = repo_root.join(".loadout").join("lock.json");
    if !lock_path.exists() {
        return Err(LoadoutError::new(
            ".loadout/lock.json is missing; run 'loadout init' first",
        ));
    }

    let lockfile: Lockfile = serde_json::from_str(&fs::read_to_string(lock_path)?)?;
    if lockfile.version != LOCKFILE_VERSION {
        return Err(LoadoutError::new(format!(
            "unsupported lockfile version: {}",
            lockfile.version
        )));
    }
    Ok(lockfile)
}

fn write_lockfile(repo_root: &Path, lockfile: &Lockfile) -> Result<()> {
    let lock_path = repo_root.join(".loadout").join("lock.json");
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(lock_path, serde_json::to_string_pretty(lockfile)? + "\n")?;
    Ok(())
}

fn load_manifest(primitive_dir: &Path) -> Result<PrimitiveManifest> {
    let manifest_path = primitive_dir.join("loadout.toml");
    if !manifest_path.exists() {
        return Err(LoadoutError::new(format!(
            "primitive manifest not found: {}",
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
        return Err(LoadoutError::new(format!(
            "unsupported primitive kind '{}'; only '{}' is supported",
            manifest.kind, SUPPORTED_KIND
        )));
    }
    if manifest.target != SUPPORTED_TARGET {
        return Err(LoadoutError::new(format!(
            "unsupported primitive target '{}'; only '{}' is supported",
            manifest.target, SUPPORTED_TARGET
        )));
    }

    let source_path = manifest.skill_source_path()?;
    if !source_path.exists() {
        return Err(LoadoutError::new(format!(
            "primitive source file not found: {}",
            source_path.display()
        )));
    }

    Ok(manifest)
}

fn validate_non_empty(field: &str, value: &str) -> Result<()> {
    if value.is_empty() {
        return Err(LoadoutError::new(format!(
            "primitive manifest field '{field}' must be a non-empty string"
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
        .join(".loadout")
        .join("baselines")
        .join(&manifest.id)
        .join("SKILL.md");

    if target_path.exists() && !lockfile.primitives.contains_key(&manifest.id) {
        return Err(LoadoutError::new(format!(
            "refusing to overwrite untracked skill at {}; remove it or track it in Loadout first",
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
        return Err(LoadoutError::new(format!(
            "baseline file missing for '{}': {}",
            primitive_id,
            baseline_path.display()
        )));
    }
    if !target_path.exists() {
        return Err(LoadoutError::new(format!(
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
