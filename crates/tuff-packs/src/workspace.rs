//! What the caller passes in: the folder to change, where baseline copies
//! go, and the one opt-in reader of the process environment.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tuff_core::error::{Result, TuffError};
use tuff_core::lockfile;

/// A project folder holding a `tuff.lock`, named by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    /// Open the project at `root`. Fails when it has no `tuff.lock`; run
    /// `tuff init` there, or [`Workspace::init`], first.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        lockfile::require_lockfile(&root)?;
        Ok(Self { root })
    }

    /// Create `root` and an empty `tuff.lock` in it when it has none, and
    /// open it. An existing lockfile is left as it is.
    pub fn init(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        let path = root.join("tuff.lock");
        if !path.exists() {
            std::fs::create_dir_all(&root)?;
            lockfile::write_lockfile_at(&path, &lockfile::Lockfile::empty())?;
        }
        Self::open(root)
    }

    /// The project folder.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Take the workspace's lock for as long as the guard lives. A second
    /// change to the same workspace, from this process or another, fails
    /// with a `refused` error until the guard is dropped.
    ///
    /// The lock is a file in the system temporary folder named after the
    /// workspace's canonical path, so nothing is added to the project.
    pub fn lock(&self) -> Result<WorkspaceLock> {
        let canonical = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone());
        let key = lockfile::hash_bytes(canonical.to_string_lossy().as_bytes());
        let path = std::env::temp_dir().join(format!("tuff-workspace-{}.lock", &key[..32]));
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .or_else(|_| File::open(&path))?;
        match file.try_lock() {
            Ok(()) => Ok(WorkspaceLock { _file: file }),
            Err(std::fs::TryLockError::WouldBlock) => Err(TuffError::refused(format!(
                "another Tuff operation is changing {}",
                self.root.display()
            ))
            .with_hint("wait for it to finish, then run this again")),
            Err(std::fs::TryLockError::Error(error)) => Err(error.into()),
        }
    }
}

/// Holds a [`Workspace`]'s lock; dropping it releases the lock.
#[derive(Debug)]
pub struct WorkspaceLock {
    _file: File,
}

/// Somewhere to keep a copy of each installed tree as it was installed,
/// keyed by its hash. `tuff diff` shows an edited file against this copy.
pub trait BaselineStore: Send + Sync {
    /// Keep the tree at `tree`, whose [`tuff_core::cache::hash_tree`] is
    /// `hash`.
    fn store(&self, hash: &str, tree: &Path) -> Result<()>;
}

/// Where installs keep their baseline copies. Edit detection does not need
/// them, since `tuff.lock` records every hash; they only give `diff` the
/// original text.
#[derive(Clone, Default)]
pub enum Baselines {
    /// A folder the caller chooses, laid out like Tuff's own cache.
    Dir(PathBuf),
    /// `.tuff-baselines/` inside the workspace, with a `.gitignore` so it
    /// is never committed.
    InWorkspace,
    /// Keep no copies.
    #[default]
    None,
    /// The caller's own storage.
    Store(Arc<dyn BaselineStore>),
}

impl std::fmt::Debug for Baselines {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dir(path) => formatter.debug_tuple("Dir").field(path).finish(),
            Self::InWorkspace => formatter.write_str("InWorkspace"),
            Self::None => formatter.write_str("None"),
            Self::Store(_) => formatter.write_str("Store(..)"),
        }
    }
}

/// The folder [`Baselines::InWorkspace`] keeps copies in.
pub const IN_WORKSPACE_BASELINES: &str = ".tuff-baselines";

impl Baselines {
    pub fn dir(path: impl Into<PathBuf>) -> Self {
        Self::Dir(path.into())
    }

    pub fn in_workspace() -> Self {
        Self::InWorkspace
    }

    pub fn none() -> Self {
        Self::None
    }

    pub fn custom(store: impl BaselineStore + 'static) -> Self {
        Self::Store(Arc::new(store))
    }

    /// Keep the tree at `tree` with hash `hash`, for a workspace at `root`.
    pub fn store(&self, root: &Path, hash: &str, tree: &Path) -> Result<()> {
        match self {
            Self::Dir(path) => {
                tuff_core::cache::populate_in(path, hash, tree)?;
            }
            Self::InWorkspace => {
                let folder = root.join(IN_WORKSPACE_BASELINES);
                std::fs::create_dir_all(&folder)?;
                let ignore = folder.join(".gitignore");
                if !ignore.exists() {
                    std::fs::write(&ignore, "*\n")?;
                }
                tuff_core::cache::populate_in(&folder, hash, tree)?;
            }
            Self::None => {}
            Self::Store(store) => store.store(hash, tree)?,
        }
        Ok(())
    }
}

/// What the `tuff` command reads from the process: the home directory and
/// the current directory. The only part of the crate that reads
/// environment variables, and only when called by name.
#[derive(Debug, Clone)]
pub struct Environment {
    /// `HOME`, when it is set.
    pub home: Option<PathBuf>,
    /// The process's current directory.
    pub current_dir: PathBuf,
}

impl Environment {
    pub fn from_process() -> Result<Self> {
        Ok(Self {
            home: std::env::var_os("HOME").map(PathBuf::from),
            current_dir: std::env::current_dir()?,
        })
    }

    /// The home directory, or a `usage` error when `HOME` is not set.
    pub fn require_home(&self) -> Result<&Path> {
        self.home
            .as_deref()
            .ok_or_else(|| TuffError::usage("HOME environment variable not set"))
    }

    /// The baseline folder the `tuff` command uses: Tuff's cache under the
    /// home directory, or `XDG_CACHE_HOME` when that is set.
    pub fn baselines(&self) -> Result<Baselines> {
        Ok(Baselines::dir(tuff_core::cache::cache_root(
            self.require_home()?,
        )))
    }

    /// The project in the current directory.
    pub fn workspace(&self) -> Result<Workspace> {
        Workspace::open(&self.current_dir)
    }
}

/// A line an operation has for the person running it. The library returns
/// these; the `tuff` command prints `Done` to standard output, and `Note`
/// (as `note: ...`) and `Warning` to standard error.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", content = "text", rename_all = "lowercase")]
pub enum Message {
    /// Something that happened, such as a file written.
    Done(String),
    /// Something worth knowing, such as a variable a server reads.
    Note(String),
    /// Something that may not behave as expected, such as a policy rule
    /// an agent enforces only in part.
    Warning(String),
}

/// Where an operation's [`Message`]s go.
pub type Messages = Vec<Message>;
