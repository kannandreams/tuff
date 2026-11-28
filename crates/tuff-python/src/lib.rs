//! The Tuff SDK for Python, `tuff_sdk`: a native module over `tuff-packs`.
//!
//! Every call runs the same Rust code as the `tuff` command. The module
//! never prints: notes and warnings are lists on the results. Calls that
//! touch git or the disk release the GIL while they run.

use std::path::PathBuf;
use std::sync::Mutex;

use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use serde::Serialize;
use tuff_core::error::{ErrorKind, TuffError as RustError};
use tuff_core::lockfile::{PackArtifactSource, PackSource};
use tuff_core::pack::{self, PackArtifact};
use tuff_packs::build::{build_for_declared_targets, build_for_targets};
use tuff_packs::git::{CheckoutRequest, checkout};
use tuff_packs::import::source_pack_of;
use tuff_packs::install::{self, Approval, PlanSummary, UpdateRequest};
use tuff_packs::workspace;

create_exception!(
    tuff_sdk,
    TuffError,
    PyException,
    "Any error from Tuff. `kind` is the same kind as the `kind` field of the CLI's JSON errors, and `hint` is the suggestion to show, or None."
);
create_exception!(tuff_sdk, UsageError, TuffError, "The request is not valid.");
create_exception!(
    tuff_sdk,
    NotFoundError,
    TuffError,
    "Something named in the request does not exist."
);
create_exception!(
    tuff_sdk,
    RefusedError,
    TuffError,
    "Tuff refused, for example because another change holds the workspace."
);
create_exception!(
    tuff_sdk,
    DriftError,
    TuffError,
    "Files changed since the plan was made, or a member has local edits."
);
create_exception!(
    tuff_sdk,
    SourceError,
    TuffError,
    "A source could not be read, such as a git address."
);
create_exception!(
    tuff_sdk,
    CorruptError,
    TuffError,
    "A file Tuff reads is damaged."
);
create_exception!(
    tuff_sdk,
    UnsupportedError,
    TuffError,
    "The request needs something Tuff does not support."
);
create_exception!(
    tuff_sdk,
    IoError,
    TuffError,
    "Reading or writing a file failed."
);
create_exception!(tuff_sdk, InternalError, TuffError, "A bug in Tuff.");
create_exception!(
    tuff_sdk,
    PlanUsedError,
    TuffError,
    "The plan was already applied; make it again."
);

/// A Rust error as the Python exception for its kind, with `kind` and
/// `hint` set.
fn py_error(error: RustError) -> PyErr {
    let message = error.message().to_string();
    let err = match error.kind() {
        ErrorKind::Usage => UsageError::new_err(message),
        ErrorKind::NotFound => NotFoundError::new_err(message),
        ErrorKind::Refused => RefusedError::new_err(message),
        ErrorKind::Drift => DriftError::new_err(message),
        ErrorKind::Source => SourceError::new_err(message),
        ErrorKind::Corrupt => CorruptError::new_err(message),
        ErrorKind::Unsupported => UnsupportedError::new_err(message),
        ErrorKind::Io => IoError::new_err(message),
        ErrorKind::Internal => InternalError::new_err(message),
    };
    Python::attach(|py| {
        let value = err.value(py);
        // Setting attributes on a fresh exception instance cannot fail.
        let _ = value.setattr("kind", error.kind().as_str());
        let _ = value.setattr("hint", error.hint());
    });
    err
}

fn plan_used() -> PyErr {
    let err = PlanUsedError::new_err("this plan was already applied; make the plan again");
    Python::attach(|py| {
        let value = err.value(py);
        let _ = value.setattr("kind", "usage");
        let _ = value.setattr("hint", "make the plan again");
    });
    err
}

trait OrPy<T> {
    fn or_py(self) -> PyResult<T>;
}

impl<T> OrPy<T> for tuff_core::error::Result<T> {
    fn or_py(self) -> PyResult<T> {
        self.map_err(py_error)
    }
}

/// A serializable value as plain Python data (dicts, lists, strings).
fn to_python(py: Python<'_>, value: &impl Serialize) -> PyResult<Py<PyAny>> {
    let text =
        serde_json::to_string(value).map_err(|error| InternalError::new_err(error.to_string()))?;
    Ok(py.import("json")?.call_method1("loads", (text,))?.unbind())
}

/// An enum that serializes as a lowercase string, as that string.
fn name_of(value: &impl Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// A project folder holding a `tuff.lock`.
#[pyclass(module = "tuff_sdk", frozen)]
struct Workspace {
    inner: workspace::Workspace,
}

/// Where to keep a copy of each installed folder, which `tuff diff` uses
/// to show the original text of an edited file.
#[pyclass(module = "tuff_sdk", frozen)]
struct Baselines {
    inner: workspace::Baselines,
}

#[pymethods]
impl Baselines {
    /// Keep the copies in a folder you choose.
    #[staticmethod]
    #[pyo3(name = "dir")]
    fn in_dir(path: PathBuf) -> Self {
        Self {
            inner: workspace::Baselines::dir(path),
        }
    }

    /// Keep the copies in `.tuff-baselines/` inside the workspace, with a
    /// `.gitignore` that keeps them out of commits.
    #[staticmethod]
    fn in_workspace() -> Self {
        Self {
            inner: workspace::Baselines::in_workspace(),
        }
    }

    /// Keep no copies. Edits are still detected, from the hashes in
    /// `tuff.lock`.
    #[staticmethod]
    fn none() -> Self {
        Self {
            inner: workspace::Baselines::none(),
        }
    }
}

/// A pack release, ready to plan: built from a folder, read from a
/// `.tuffpack` file, or checked out from git.
#[pyclass(module = "tuff_sdk", frozen)]
struct Pack {
    artifact: PackArtifact,
    source: PackSource,
    inspected: serde_json::Value,
    notes: Vec<String>,
    warnings: Vec<String>,
}

#[pymethods]
impl Pack {
    /// Build the source pack in `folder` for `targets`, or for the
    /// harnesses its `tuff-pack.toml` declares.
    #[staticmethod]
    #[pyo3(signature = (folder, targets = None))]
    fn build(py: Python<'_>, folder: PathBuf, targets: Option<Vec<String>>) -> PyResult<Self> {
        let built = py
            .detach(|| match &targets {
                Some(targets) => build_for_targets(&folder, targets),
                None => build_for_declared_targets(&folder),
            })
            .or_py()?;
        Ok(Self {
            artifact: built.artifact,
            source: PackSource::Artifact(PackArtifactSource { registry: None }),
            inspected: serde_json::json!({ "kind": "folder", "path": folder.to_string_lossy() }),
            notes: built.notes,
            warnings: built.warnings,
        })
    }

    /// Read a built `.tuffpack` file.
    #[staticmethod]
    fn read(py: Python<'_>, path: PathBuf) -> PyResult<Self> {
        let artifact = py.detach(|| pack::read_artifact(&path)).or_py()?;
        Ok(Self {
            artifact,
            source: PackSource::Artifact(PackArtifactSource { registry: None }),
            inspected: serde_json::json!({ "kind": "artifact", "path": path.to_string_lossy() }),
            notes: Vec::new(),
            warnings: Vec::new(),
        })
    }

    /// Check out a pack, or a Claude Code plugin read as a pack, from a git
    /// address, and build it for `targets`. `ref` is a tag, branch, or
    /// commit; the newest release tag when None. Uses the `git` program on
    /// the machine, with its settings and credentials.
    #[staticmethod]
    #[pyo3(signature = (url, targets, r#ref = None, path = None, plugin = None))]
    fn checkout(
        py: Python<'_>,
        url: String,
        targets: Vec<String>,
        r#ref: Option<String>,
        path: Option<String>,
        plugin: Option<String>,
    ) -> PyResult<Self> {
        py.detach(|| {
            let checked_out = checkout(CheckoutRequest {
                url: &url,
                path: path.as_deref(),
                git_ref: r#ref.as_deref(),
                plugin: plugin.as_deref(),
                allow_remote: false,
            })?;
            let source_pack = source_pack_of(&checked_out)?;
            let built = build_for_targets(&source_pack.root, &targets)?;
            let mut inspected = serde_json::to_value(&checked_out.source)?;
            inspected["kind"] = "git".into();
            let mut notes = source_pack.notes;
            notes.extend(built.notes);
            Ok(Self {
                artifact: built.artifact,
                source: PackSource::Git(checked_out.source.clone()),
                inspected,
                notes,
                warnings: built.warnings,
            })
        })
        .or_py()
    }

    /// The pack's name, such as `com.example/review`.
    #[getter]
    fn name(&self) -> &str {
        &self.artifact.metadata.name
    }

    #[getter]
    fn version(&self) -> &str {
        &self.artifact.metadata.version
    }

    #[getter]
    fn description(&self) -> &str {
        &self.artifact.metadata.description
    }

    /// The artifact's digest: the same digest means the same pack content.
    #[getter]
    fn digest(&self) -> &str {
        &self.artifact.digest
    }

    /// Notes from building or importing, such as a member a harness leaves out.
    #[getter]
    fn notes(&self) -> Vec<String> {
        self.notes.clone()
    }

    #[getter]
    fn warnings(&self) -> Vec<String> {
        self.warnings.clone()
    }

    /// The pack described for a review screen: the same document as
    /// `tuff pack inspect --json`, as a dict.
    fn inspect(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let document = tuff_packs::inspect::document(&self.artifact, &self.inspected).or_py()?;
        to_python(py, &document)
    }

    fn __repr__(&self) -> String {
        format!(
            "<Pack {} {}>",
            self.artifact.metadata.name, self.artifact.metadata.version
        )
    }
}

/// A release of a pack.
#[pyclass(module = "tuff_sdk", frozen, get_all)]
struct Release {
    version: String,
    digest: String,
}

#[pymethods]
impl Release {
    fn __repr__(&self) -> String {
        format!("<Release {} {}>", self.version, self.digest)
    }
}

/// One file a plan creates, modifies, or deletes.
#[pyclass(module = "tuff_sdk", frozen, get_all)]
struct PlanFile {
    /// Relative to the workspace, with `/` separators.
    path: String,
    /// `"create"`, `"modify"`, or `"delete"`.
    change: String,
}

#[pymethods]
impl PlanFile {
    fn __repr__(&self) -> String {
        format!("<PlanFile {} {}>", self.change, self.path)
    }
}

/// One member of the pack and what the plan does to it.
#[pyclass(module = "tuff_sdk", frozen, get_all)]
struct PlanMember {
    id: String,
    /// `"skill"`, `"mcp-server"`, `"hook"`, and so on.
    #[pyo3(name = "type")]
    capability_type: String,
    /// The version it moves to; None for a member being removed.
    version: Option<String>,
    /// `"add"`, `"update"`, or `"remove"`.
    change: String,
}

#[pymethods]
impl PlanMember {
    fn __repr__(&self) -> String {
        format!("<PlanMember {} {}>", self.change, self.id)
    }
}

fn release(release: Option<&install::PlanRelease>) -> Option<Release> {
    release.map(|release| Release {
        version: release.version.clone(),
        digest: release.digest.clone(),
    })
}

/// The getters shared by a plan and the summary of an applied plan.
macro_rules! summary_class {
    ($name:ident, $summary:ident, $what:literal, { $($extra:tt)* }) => {
        #[pymethods]
        impl $name {
            /// `"install"`, `"update"`, or `"remove"`.
            #[getter]
            fn operation(&self) -> String {
                name_of(&self.$summary().operation)
            }

            /// The pack's name.
            #[getter]
            fn pack(&self) -> String {
                self.$summary().pack.clone()
            }

            /// The release installed before; None for an install.
            #[getter]
            fn installed(&self) -> Option<Release> {
                release(self.$summary().installed.as_ref())
            }

            /// The release the plan installs; None for a removal.
            #[getter]
            fn release(&self) -> Option<Release> {
                release(self.$summary().release.as_ref())
            }

            #[getter]
            fn targets(&self) -> Vec<String> {
                self.$summary().targets.clone()
            }

            #[getter]
            fn members(&self) -> Vec<PlanMember> {
                self.$summary()
                    .members
                    .iter()
                    .map(|member| PlanMember {
                        id: member.id.clone(),
                        capability_type: name_of(&member.capability_type),
                        version: member.version.clone(),
                        change: name_of(&member.change),
                    })
                    .collect()
            }

            /// Every file the plan writes or removes, `tuff.lock` included,
            /// sorted by path.
            #[getter]
            fn files(&self) -> Vec<PlanFile> {
                self.$summary()
                    .files
                    .iter()
                    .map(|file| PlanFile {
                        path: file.path.clone(),
                        change: name_of(&file.change),
                    })
                    .collect()
            }

            /// Notes to show before asking whether to apply, such as a hook
            /// that runs code.
            #[getter]
            fn notes(&self) -> Vec<String> {
                self.$summary().notes.clone()
            }

            /// Identifies exactly this change. Two plans with the same
            /// fingerprint make the same change.
            #[getter]
            fn fingerprint(&self) -> String {
                self.$summary().fingerprint.clone()
            }

            /// The same document as the Rust library's JSON, as a dict.
            fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
                to_python(py, self.$summary())
            }

            /// The same document as the Rust library's JSON.
            fn to_json(&self) -> PyResult<String> {
                serde_json::to_string(self.$summary())
                    .map_err(|error| InternalError::new_err(error.to_string()))
            }

            fn __repr__(&self) -> String {
                let summary = self.$summary();
                format!(
                    "<{} {} {} ({} files)>",
                    $what,
                    name_of(&summary.operation),
                    summary.pack,
                    summary.files.len()
                )
            }

            $($extra)*
        }
    };
}

/// A change to a workspace, worked out and not yet written. Nothing is
/// written until it is passed to `Workspace.apply` or
/// `Workspace.apply_approved`, and it can be applied once.
#[pyclass(module = "tuff_sdk", frozen)]
struct Plan {
    summary: PlanSummary,
    work: Mutex<Option<install::Plan>>,
}

impl Plan {
    fn new(plan: install::Plan) -> Self {
        Self {
            summary: plan.summary().clone(),
            work: Mutex::new(Some(plan)),
        }
    }

    fn summary(&self) -> &PlanSummary {
        &self.summary
    }

    fn take(&self) -> PyResult<install::Plan> {
        self.work
            .lock()
            .map_err(|_| InternalError::new_err("a plan's lock was poisoned"))?
            .take()
            .ok_or_else(plan_used)
    }
}

summary_class!(Plan, summary, "Plan", {
    /// Whether the plan was applied, or handed to `apply_approved`.
    #[getter]
    fn used(&self) -> bool {
        self.work.lock().map(|work| work.is_none()).unwrap_or(true)
    }
});

/// What an applied plan changed.
#[pyclass(module = "tuff_sdk", frozen)]
struct Summary {
    summary: PlanSummary,
}

impl Summary {
    fn summary(&self) -> &PlanSummary {
        &self.summary
    }
}

summary_class!(Summary, summary, "Summary", {});

/// `Workspace.apply_approved` wrote the plan.
#[pyclass(module = "tuff_sdk", frozen, get_all)]
struct Applied {
    summary: Py<Summary>,
}

/// `Workspace.apply_approved` found a different change from the one
/// approved, and wrote nothing. Show `plan` and ask again.
#[pyclass(module = "tuff_sdk", frozen, get_all)]
struct Changed {
    plan: Py<Plan>,
}

#[pymethods]
impl Workspace {
    /// Open a project folder that holds a `tuff.lock`.
    #[staticmethod]
    fn open(path: PathBuf) -> PyResult<Self> {
        Ok(Self {
            inner: workspace::Workspace::open(path).or_py()?,
        })
    }

    /// Create the folder, when missing, and an empty `tuff.lock` in it.
    #[staticmethod]
    fn init(path: PathBuf) -> PyResult<Self> {
        Ok(Self {
            inner: workspace::Workspace::init(path).or_py()?,
        })
    }

    #[getter]
    fn root(&self) -> PathBuf {
        self.inner.root().to_path_buf()
    }

    /// Plan installing every member of `pack` for `targets`. Writes nothing.
    fn plan_install(&self, py: Python<'_>, pack: &Pack, targets: Vec<String>) -> PyResult<Plan> {
        let plan = py
            .detach(|| install::plan_install(&self.inner, &pack.artifact, &pack.source, &targets))
            .or_py()?;
        Ok(Plan::new(plan))
    }

    /// Plan moving the installed pack `name` to `pack`. Returns None when
    /// `pack` is the installed release. Refuses a member with local edits
    /// unless `force` is set. `targets` defaults to every harness the pack
    /// is installed for.
    #[pyo3(signature = (name, pack, targets = None, force = false))]
    fn plan_update(
        &self,
        py: Python<'_>,
        name: String,
        pack: &Pack,
        targets: Option<Vec<String>>,
        force: bool,
    ) -> PyResult<Option<Plan>> {
        let targets = targets.unwrap_or_default();
        let plan = py
            .detach(|| {
                install::plan_update(
                    &self.inner,
                    UpdateRequest {
                        pack: &name,
                        artifact: &pack.artifact,
                        source: &pack.source,
                        targets: &targets,
                        force,
                    },
                )
            })
            .or_py()?;
        Ok(plan.map(Plan::new))
    }

    /// Plan removing the installed pack `name` and everything it installed.
    #[pyo3(signature = (name, force = false))]
    fn plan_remove(&self, py: Python<'_>, name: String, force: bool) -> PyResult<Plan> {
        let plan = py
            .detach(|| install::plan_remove(&self.inner, &name, force))
            .or_py()?;
        Ok(Plan::new(plan))
    }

    /// Write `plan` as one change. Raises DriftError when a file it touches
    /// changed after it was made, and RefusedError while another change
    /// holds the workspace. `baselines` defaults to `Baselines.none()`.
    #[pyo3(signature = (plan, baselines = None))]
    fn apply(
        &self,
        py: Python<'_>,
        plan: &Plan,
        baselines: Option<&Baselines>,
    ) -> PyResult<Summary> {
        let work = plan.take()?;
        let none = workspace::Baselines::none();
        let baselines = baselines.map_or(&none, |baselines| &baselines.inner);
        let summary = py
            .detach(|| install::apply(&self.inner, baselines, work))
            .or_py()?;
        Ok(Summary { summary })
    }

    /// Write `plan` only when its fingerprint is `approved`, the fingerprint
    /// kept with an earlier approval. Returns `Applied`, or `Changed` with
    /// the new plan and nothing written.
    #[pyo3(signature = (plan, approved, baselines = None))]
    fn apply_approved(
        &self,
        py: Python<'_>,
        plan: &Plan,
        approved: String,
        baselines: Option<&Baselines>,
    ) -> PyResult<Py<PyAny>> {
        let work = plan.take()?;
        let none = workspace::Baselines::none();
        let baselines = baselines.map_or(&none, |baselines| &baselines.inner);
        let approval = py
            .detach(|| install::apply_approved(&self.inner, baselines, work, &approved))
            .or_py()?;
        match approval {
            Approval::Applied(summary) => {
                let summary = Py::new(py, Summary { summary })?;
                Ok(Py::new(py, Applied { summary })?.into_any())
            }
            Approval::Changed(plan) => {
                let plan = Py::new(py, Plan::new(plan))?;
                Ok(Py::new(py, Changed { plan })?.into_any())
            }
        }
    }

    fn __repr__(&self) -> String {
        format!("<Workspace {}>", self.inner.root().display())
    }
}

#[pymodule]
fn tuff_sdk(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = module.py();
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    module.add_class::<Workspace>()?;
    module.add_class::<Baselines>()?;
    module.add_class::<Pack>()?;
    module.add_class::<Plan>()?;
    module.add_class::<Summary>()?;
    module.add_class::<PlanFile>()?;
    module.add_class::<PlanMember>()?;
    module.add_class::<Release>()?;
    module.add_class::<Applied>()?;
    module.add_class::<Changed>()?;
    module.add("TuffError", py.get_type::<TuffError>())?;
    module.add("UsageError", py.get_type::<UsageError>())?;
    module.add("NotFoundError", py.get_type::<NotFoundError>())?;
    module.add("RefusedError", py.get_type::<RefusedError>())?;
    module.add("DriftError", py.get_type::<DriftError>())?;
    module.add("SourceError", py.get_type::<SourceError>())?;
    module.add("CorruptError", py.get_type::<CorruptError>())?;
    module.add("UnsupportedError", py.get_type::<UnsupportedError>())?;
    module.add("IoError", py.get_type::<IoError>())?;
    module.add("InternalError", py.get_type::<InternalError>())?;
    module.add("PlanUsedError", py.get_type::<PlanUsedError>())?;
    Ok(())
}
