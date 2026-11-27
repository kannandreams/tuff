//! Harness bundle formats (RFC-110 D9).
//!
//! A bundle format is a harness's own layout for a set of capabilities
//! installed as one unit, such as a Claude Code plugin. An adapter crate
//! declares the format; this module holds what every format shares: the
//! contract a reader implements, the pack a reader produces, and the writer
//! that turns that pack into an ordinary standalone source pack on disk.
//! After an import, nothing in Tuff depends on where a pack came from.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::error::{Result, TuffError};
use crate::manifest::CapabilityType;
use crate::pack::{self, PackManifest, PackMember, PackUnsupported};

/// One file of a bundle, by its path relative to the bundle's root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

/// One capability read from a bundle.
#[derive(Debug, Clone)]
pub struct ImportedMember {
    pub id: String,
    pub capability_type: CapabilityType,
    /// The member's `tuff.toml`.
    pub manifest: toml::Table,
    /// The member's own files, relative to its folder.
    pub files: Vec<BundleFile>,
}

/// A bundle read as a pack.
#[derive(Debug, Clone)]
pub struct ImportedPack {
    /// The pack manifest. Its `capabilities` are filled in by the writer.
    pub manifest: PackManifest,
    pub members: Vec<ImportedMember>,
    /// Files the pack does not carry as members, kept under `extra/` so a
    /// later Tuff or a host can use them.
    pub extra: Vec<BundleFile>,
    /// Facts worth telling the person importing, such as a version Tuff had
    /// to choose.
    pub notes: Vec<String>,
}

impl ImportedPack {
    /// What the source had that the pack does not carry.
    pub fn unsupported(&self) -> &[PackUnsupported] {
        self.manifest
            .import
            .as_ref()
            .map(|import| import.unsupported.as_slice())
            .unwrap_or_default()
    }
}

/// Where a bundle's files are: in the folder that was read, or in another
/// repository that a listing (such as a marketplace) points to.
#[derive(Debug, Clone)]
pub enum BundleLocation {
    Local(LocalBundle),
    Remote(RemoteBundle),
}

/// A bundle whose files are in a local folder.
#[derive(Debug, Clone)]
pub struct LocalBundle {
    pub root: PathBuf,
    /// The bundle's name in the listing that selected it, if one did.
    pub name: Option<String>,
    /// The listing's entry, for formats where it can declare components.
    pub entry: Option<serde_json::Value>,
}

/// A bundle a listing places in another git repository.
#[derive(Debug, Clone)]
pub struct RemoteBundle {
    pub name: String,
    pub url: String,
    /// The folder inside that repository.
    pub path: Option<String>,
    /// A tag or branch the listing names.
    pub git_ref: Option<String>,
    /// A commit the listing names.
    pub commit: Option<String>,
    pub entry: serde_json::Value,
}

/// What an adapter crate implements to read its harness's bundle format.
pub trait BundleReader {
    /// The format id, such as `claude-plugin`.
    fn id(&self) -> &'static str;
    /// Whether `root` holds a bundle or a listing of bundles in this format.
    fn detect(&self, root: &Path) -> bool;
    /// Find the bundle at `root`, or the one named `entry` in a listing.
    ///
    /// # Errors
    ///
    /// Returns an error when no bundle is found, or a listing holds several
    /// and none is named.
    fn locate(&self, root: &Path, entry: Option<&str>) -> Result<BundleLocation>;
    /// Read a local bundle as a pack. `version_hint` is the version a release
    /// tag named, for a bundle that declares none.
    ///
    /// # Errors
    ///
    /// Returns an error for a malformed bundle. Parts Tuff cannot carry are
    /// recorded as unsupported, not refused.
    fn read(&self, bundle: &LocalBundle, version_hint: Option<&str>) -> Result<ImportedPack>;
}

/// Every regular file under `relative` in `root`, with paths relative to
/// `root`. Symbolic links are never followed; each one is returned in the
/// second list so the caller can report it. `.git` folders are skipped.
///
/// # Errors
///
/// Returns an error when a directory cannot be read.
pub fn read_tree(root: &Path, relative: &Path) -> Result<(Vec<BundleFile>, Vec<String>)> {
    let mut files = Vec::new();
    let mut links = Vec::new();
    walk(root, &root.join(relative), &mut files, &mut links)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    links.sort();
    Ok((files, links))
}

fn walk(
    root: &Path,
    current: &Path,
    files: &mut Vec<BundleFile>,
    links: &mut Vec<String>,
) -> Result<()> {
    let mut entries = fs::read_dir(current)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        if entry.file_name() == ".git" {
            continue;
        }
        let relative = relative_name(root, &path)?;
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            links.push(relative);
        } else if metadata.is_dir() {
            walk(root, &path, files, links)?;
        } else if metadata.is_file() {
            files.push(BundleFile {
                path: relative,
                bytes: fs::read(&path)?,
            });
        }
    }
    Ok(())
}

fn relative_name(root: &Path, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| TuffError::refused("bundle file escaped its root"))?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

/// Read a path a bundle manifest declares, such as `./skills/review`, as a
/// clean path inside the bundle. `.` is the bundle's root.
///
/// # Errors
///
/// Returns a refusal for an absolute path or one that climbs out.
pub fn bundle_path(declared: &str) -> Result<PathBuf> {
    let trimmed = declared.trim_start_matches("./");
    if trimmed.is_empty() || trimmed == "." {
        return Ok(PathBuf::new());
    }
    let path = Path::new(trimmed);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(TuffError::refused(format!(
            "bundle path '{declared}' leaves the bundle"
        )));
    }
    Ok(path.to_path_buf())
}

/// Write an imported pack as a standalone source pack in `output`, which
/// must be missing or empty: `tuff-pack.toml`, `capabilities/<id>/` for
/// each member, and `extra/` for everything else. The folder is assembled
/// in a temporary directory beside `output` and renamed into place.
///
/// # Errors
///
/// Returns an error for a non-empty output, an unsafe or duplicate path, or
/// a failed write.
pub fn write_source_pack(output: &Path, imported: &ImportedPack) -> Result<()> {
    if output.exists() && (!output.is_dir() || fs::read_dir(output)?.next().is_some()) {
        return Err(TuffError::refused(format!(
            "import output must be missing or empty: {}",
            output.display()
        )));
    }
    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix("tuff-import-")
        .tempdir_in(parent)?;

    let mut manifest = imported.manifest.clone();
    manifest.capabilities.clear();
    let mut ids = BTreeSet::new();
    for member in &imported.members {
        crate::manifest::validate_capability_id(&member.id)?;
        if !ids.insert(member.id.clone()) {
            return Err(TuffError::usage(format!(
                "two imported members are both named '{}'",
                member.id
            )));
        }
        let folder = PathBuf::from("capabilities").join(&member.id);
        write_file(
            staging.path(),
            &folder.join("tuff.toml"),
            toml::to_string_pretty(&member.manifest)?.as_bytes(),
        )?;
        for file in &member.files {
            if file.path == "tuff.toml" {
                return Err(TuffError::refused(format!(
                    "imported member '{}' has its own tuff.toml, which Tuff writes",
                    member.id
                )));
            }
            write_file(
                staging.path(),
                &folder.join(pack::validate_relative_path(Path::new(&file.path))?),
                &file.bytes,
            )?;
        }
        manifest.capabilities.push(PackMember {
            path: format!("capabilities/{}", member.id),
        });
    }
    for file in &imported.extra {
        write_file(
            staging.path(),
            &Path::new("extra").join(pack::validate_relative_path(Path::new(&file.path))?),
            &file.bytes,
        )?;
    }
    pack::write_manifest(&staging.path().join(pack::PACK_MANIFEST_FILE), &manifest)?;
    // Loading the written folder is the check that it is a valid source pack.
    pack::load_pack(staging.path())?;

    if output.exists() {
        fs::remove_dir(output)?;
    }
    fs::rename(staging.keep(), output)?;
    Ok(())
}

fn write_file(root: &Path, relative: &Path, bytes: &[u8]) -> Result<()> {
    let destination = root.join(relative);
    if destination.exists() {
        return Err(TuffError::usage(format!(
            "imported pack writes {} twice",
            relative.display()
        )));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(destination, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_declared_path_stays_inside_the_bundle() {
        assert_eq!(
            bundle_path("./skills/review").unwrap(),
            PathBuf::from("skills/review")
        );
        assert_eq!(bundle_path(".").unwrap(), PathBuf::new());
        assert_eq!(bundle_path("./").unwrap(), PathBuf::new());
        for escape in ["../outside", "./a/../../b", "/etc"] {
            assert!(bundle_path(escape).is_err(), "{escape}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn read_tree_reports_links_without_following_them() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("skills/a")).unwrap();
        fs::write(temp.path().join("skills/a/SKILL.md"), "# A\n").unwrap();
        std::os::unix::fs::symlink("/etc/hosts", temp.path().join("skills/a/hosts")).unwrap();
        fs::create_dir_all(temp.path().join(".git")).unwrap();
        fs::write(temp.path().join(".git/HEAD"), "ref").unwrap();
        let (files, links) = read_tree(temp.path(), Path::new("")).unwrap();
        assert_eq!(
            files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            ["skills/a/SKILL.md"]
        );
        assert_eq!(links, ["skills/a/hosts"]);
    }
}
