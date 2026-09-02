use std::path::Path;

use semver::Version;

use crate::error::Result;
use crate::git;
use crate::lockfile;
use crate::manifest::CapabilityType;
use crate::oci::{self, OciTransferOptions};

use super::{home_dir_opt, render_table, short_sha, style_outdated_status};

struct OutdatedRow {
    id: String,
    capability_type: CapabilityType,
    target: String,
    current: String,
    latest: String,
    status: String,
}

/// What the registry's tags say about an installed pack version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PackVersionStatus {
    /// A newer semver tag is published.
    Newer(String),
    /// The installed version is the newest semver tag.
    Current,
    /// Nothing conclusive: the installed version or every tag is unparsable.
    Unknown,
}

/// Compare an installed pack version against the tags published for it.
///
/// Tags are not compared as strings: `"1.9.0" > "1.10.0"` under plain
/// lexicographic ordering, which is the wrong answer and exactly the kind of
/// confidently-wrong result `tuff outdated` should not produce. Only tags
/// that parse as semver are compared, using real version ordering; anything
/// else is excluded rather than guessed at. `tuff pack build --version` does
/// not enforce a version scheme, so an unparsable installed version is a
/// real case, not a hypothetical one.
pub(crate) fn compare_pack_versions(installed: &str, tags: &[String]) -> PackVersionStatus {
    let Ok(installed_version) = Version::parse(installed) else {
        return PackVersionStatus::Unknown;
    };
    match tags.iter().filter_map(|tag| Version::parse(tag).ok()).max() {
        None => PackVersionStatus::Unknown,
        Some(latest) if latest > installed_version => PackVersionStatus::Newer(latest.to_string()),
        Some(_) => PackVersionStatus::Current,
    }
}

/// Whether the installed pack tag still names the bytes that were installed.
///
/// A tag is mutable by design. `PackProvenance.digest` records the artifact
/// that was actually installed, and the tag's manifest names the artifact it
/// points at now; when they differ, someone published different bytes under
/// the same version. That is a different finding from "a newer version
/// exists": it means the version you have may not be what you think it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PackTagIntegrity {
    /// The tag still points at the installed artifact.
    Matches,
    /// The tag now points at a different artifact.
    Repointed { live_digest: String },
    /// The registry no longer has the tag at all.
    Missing,
    /// The tag could not be resolved (network, auth, or protocol failure).
    Unavailable(String),
}

/// Resolve the installed tag and compare it against the recorded digest.
pub(crate) fn verify_pack_tag(
    pack: &lockfile::PackProvenance,
    registry: &str,
    options: &OciTransferOptions,
) -> PackTagIntegrity {
    let reference = format!("{registry}:{}", pack.version);
    match super::block_on_oci(oci::resolve_pack_tag(&reference, options)) {
        Err(error) => PackTagIntegrity::Unavailable(error.to_string()),
        Ok(None) => PackTagIntegrity::Missing,
        Ok(Some(resolved)) => {
            if resolved.artifact_digest == format!("sha256:{}", pack.digest) {
                PackTagIntegrity::Matches
            } else {
                PackTagIntegrity::Repointed {
                    live_digest: resolved.artifact_digest,
                }
            }
        }
    }
}

/// Render [`compare_pack_versions`] as an `outdated` table row.
fn pick_latest_pack_version(installed: &str, tags: &[String]) -> (String, String, String) {
    let current = installed.to_string();
    match compare_pack_versions(installed, tags) {
        PackVersionStatus::Unknown => (current, "—".to_string(), "not checked".to_string()),
        PackVersionStatus::Newer(latest) => (current, latest, "outdated".to_string()),
        PackVersionStatus::Current => (current.clone(), current, "up to date".to_string()),
    }
}

/// Fold the tag integrity into a version row. Integrity findings win: a
/// repointed or deleted tag is reported even when a newer version exists,
/// because "what you have is not what you installed" changes what the
/// LATEST column means. LATEST is still shown, so the way forward is visible.
fn pack_row(
    installed: &str,
    tags: &[String],
    integrity: &PackTagIntegrity,
) -> (String, String, String) {
    let (current, latest, status) = pick_latest_pack_version(installed, tags);
    let status = match integrity {
        PackTagIntegrity::Matches => status,
        PackTagIntegrity::Repointed { .. } => "repointed".to_string(),
        PackTagIntegrity::Missing => "tag missing".to_string(),
        PackTagIntegrity::Unavailable(_) => "error".to_string(),
    };
    (current, latest, status)
}

/// One registry round trip per pack per run, not per member per harness.
///
/// Every member of a pack shares the same provenance, and `outdated` emits a
/// row per member per target, so without this a four-member pack installed
/// for two harnesses would list the registry's tags eight times.
#[derive(Default)]
pub(crate) struct PackCheckCache {
    rows: std::collections::BTreeMap<String, (String, String, String)>,
}

/// Check a pack-sourced capability against its registry.
///
/// `pack.registry` is only present when the pack was installed with
/// `tuff add pack --reference`; the caller already confirmed that before
/// reaching here.
fn classify_pack(
    pack: &lockfile::PackProvenance,
    registry: &str,
    options: &OciTransferOptions,
    cache: &mut PackCheckCache,
) -> (String, String, String) {
    let key = format!("{registry}:{}@{}", pack.version, pack.digest);
    if let Some(row) = cache.rows.get(&key) {
        return row.clone();
    }
    let row = match super::block_on_oci(oci::list_pack_versions(registry, options)) {
        Err(_) => (
            pack.version.clone(),
            "unavailable".to_string(),
            "error".to_string(),
        ),
        Ok(tags) => {
            let integrity = verify_pack_tag(pack, registry, options);
            pack_row(&pack.version, &tags, &integrity)
        }
    };
    cache.rows.insert(key, row.clone());
    row
}

/// Classify one capability against its upstream.
///
/// A capability with no recorded source and no registry-aware pack — anything
/// installed from a pack without `--reference`, or from a local path —
/// cannot be checked at all. That is reported as `not checked` rather than
/// folded into `up to date`: an honest unknown sends someone to look, a
/// confident "up to date" does not.
fn classify(
    entry: &lockfile::CapabilityLockEntry,
    oci_options: &OciTransferOptions,
    cache: &mut PackCheckCache,
) -> (String, String, String) {
    if let Some(pack) = &entry.pack
        && let Some(registry) = &pack.registry
    {
        return classify_pack(pack, registry, oci_options, cache);
    }

    let Some(src) = &entry.source else {
        return (
            entry.installed_version.clone(),
            "—".to_string(),
            "not checked".to_string(),
        );
    };

    if src.source_type == crate::catalog::SOURCE_TYPE {
        let (latest, status) = match crate::catalog::lookup(&src.skill) {
            Ok(Some(manifest)) if manifest.version == entry.installed_version => {
                (manifest.version, "up to date")
            }
            Ok(Some(manifest)) => (manifest.version, "outdated"),
            // Removed from the catalog, or the catalog itself is broken:
            // either way there is nothing current to compare against.
            _ => ("unavailable".to_string(), "error"),
        };
        return (entry.installed_version.clone(), latest, status.to_string());
    }

    match git::clone_to_temp(&src.url, None).and_then(|(_guard, d, _)| git::resolve_ref(&d)) {
        Err(_) => (
            short_sha(&entry.installed_version).to_string(),
            "unavailable".to_string(),
            "error".to_string(),
        ),
        Ok(latest_sha) => {
            let status = if latest_sha == entry.installed_version {
                "up to date"
            } else {
                "outdated"
            };
            (
                short_sha(&entry.installed_version).to_string(),
                short_sha(&latest_sha).to_string(),
                status.to_string(),
            )
        }
    }
}

fn collect_rows(
    lf: &lockfile::Lockfile,
    oci_options: &OciTransferOptions,
    cache: &mut PackCheckCache,
    rows: &mut Vec<OutdatedRow>,
) {
    for (id, entry) in &lf.capabilities {
        for target_id in entry.targets.keys() {
            let (current, latest, status) = classify(entry, oci_options, cache);
            rows.push(OutdatedRow {
                id: id.clone(),
                capability_type: entry.capability_type,
                target: target_id.clone(),
                current,
                latest,
                status,
            });
        }
    }
}

pub fn cmd_outdated(
    repo_root: &Path,
    plain_http: bool,
    ca_files: &[std::path::PathBuf],
) -> Result<()> {
    let oci_options = OciTransferOptions {
        plain_http,
        ca_files: ca_files.to_vec(),
    };
    let mut rows: Vec<OutdatedRow> = Vec::new();
    let mut cache = PackCheckCache::default();

    if let Ok(lf) = lockfile::require_lockfile(repo_root) {
        collect_rows(&lf, &oci_options, &mut cache, &mut rows);
    }

    if let Some(home) = home_dir_opt() {
        let lock_path = crate::paths::global_lockfile(&home);
        if let Ok(lf) = lockfile::read_lockfile_at(&lock_path) {
            collect_rows(&lf, &oci_options, &mut cache, &mut rows);
        }
    }

    if rows.is_empty() {
        println!("no capabilities installed");
        return Ok(());
    }

    rows.sort_by(|a, b| a.id.cmp(&b.id));

    let table_rows: Vec<Vec<String>> = rows
        .into_iter()
        .map(|r| {
            let status = style_outdated_status(&r.status);
            let row = OutdatedRow { status, ..r };
            vec![
                row.id,
                row.capability_type.to_string(),
                row.target,
                row.current,
                row.latest,
                row.status,
            ]
        })
        .collect();

    println!(
        "{}",
        render_table(
            &["ID", "TYPE", "AGENT", "CURRENT", "LATEST", "STATUS"],
            &table_rows
        )
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn entry(source: Option<lockfile::SourceMetadata>) -> lockfile::CapabilityLockEntry {
        lockfile::CapabilityLockEntry {
            capability_type: CapabilityType::Skill,
            installed_version: "1.0.0".to_string(),
            description: String::new(),
            source_path: "agent-capabilities/example".to_string(),
            targets: BTreeMap::new(),
            source,
            scope: "project".to_string(),
            pack: None,
            implementation: None,
            parameters: None,
            workflow: None,
            server: None,
        }
    }

    #[test]
    fn a_capability_with_no_source_is_reported_as_unchecked() {
        // Anything installed from a pack without --reference, or from a
        // local path, has no git source and no registry to check. Reporting
        // "up to date" here states a conclusion that was never reached: the
        // row would claim the capability is current while LATEST is unknown.
        let (current, latest, status) = classify(
            &entry(None),
            &OciTransferOptions::default(),
            &mut PackCheckCache::default(),
        );

        assert_eq!(status, "not checked");
        assert_ne!(
            status, "up to date",
            "an unchecked capability must not read as current"
        );
        assert_eq!(latest, "—");
        assert_eq!(current, "1.0.0");
    }

    #[test]
    fn a_newer_semver_tag_is_reported_as_outdated() {
        let tags = vec![
            "1.0.0".to_string(),
            "1.2.0".to_string(),
            "0.9.0".to_string(),
        ];
        let (current, latest, status) = pick_latest_pack_version("1.0.0", &tags);

        assert_eq!(status, "outdated");
        assert_eq!(current, "1.0.0");
        assert_eq!(latest, "1.2.0");
    }

    #[test]
    fn the_installed_tag_being_the_newest_reads_as_up_to_date() {
        let tags = vec!["1.0.0".to_string(), "0.9.0".to_string()];
        let (_, _, status) = pick_latest_pack_version("1.0.0", &tags);

        assert_eq!(status, "up to date");
    }

    #[test]
    fn version_ordering_is_semantic_not_lexicographic() {
        // Plain string comparison puts "1.9.0" after "1.10.0", because '9' >
        // '1' as the first differing character. That is the wrong answer,
        // and exactly the shape of bug this function exists to avoid.
        let tags = vec!["1.10.0".to_string()];
        let (_, latest, status) = pick_latest_pack_version("1.9.0", &tags);

        assert_eq!(status, "outdated");
        assert_eq!(latest, "1.10.0");
    }

    #[test]
    fn tags_that_do_not_parse_as_semver_are_excluded_not_guessed_at() {
        let tags = vec!["latest".to_string(), "nightly".to_string()];
        let (_, latest, status) = pick_latest_pack_version("1.0.0", &tags);

        assert_eq!(status, "not checked");
        assert_eq!(latest, "—");
    }

    #[test]
    fn an_unparsable_installed_version_is_reported_as_unchecked() {
        // tuff pack build --version does not enforce a version scheme, so
        // this is a real case: the pack itself may not be semver-tagged.
        let tags = vec!["1.0.0".to_string()];
        let (current, latest, status) = pick_latest_pack_version("release-42", &tags);

        assert_eq!(status, "not checked");
        assert_eq!(latest, "—");
        assert_eq!(current, "release-42");
    }

    #[test]
    fn a_repointed_tag_is_reported_even_when_a_newer_version_exists() {
        // "A newer version exists" and "the version you have is not what
        // you installed" are different findings; the second must not hide
        // behind the first. LATEST still shows the way forward.
        let tags = vec!["1.0.0".to_string(), "1.2.0".to_string()];
        let integrity = PackTagIntegrity::Repointed {
            live_digest: "sha256:beef".to_string(),
        };
        let (current, latest, status) = pack_row("1.0.0", &tags, &integrity);

        assert_eq!(status, "repointed");
        assert_eq!(current, "1.0.0");
        assert_eq!(latest, "1.2.0");
    }

    #[test]
    fn a_repointed_tag_never_reads_as_up_to_date() {
        let tags = vec!["1.0.0".to_string()];
        let integrity = PackTagIntegrity::Repointed {
            live_digest: "sha256:beef".to_string(),
        };
        let (_, _, status) = pack_row("1.0.0", &tags, &integrity);

        assert_eq!(status, "repointed");
    }

    #[test]
    fn a_deleted_tag_is_its_own_status() {
        let tags = vec!["1.2.0".to_string()];
        let (_, latest, status) = pack_row("1.0.0", &tags, &PackTagIntegrity::Missing);

        assert_eq!(status, "tag missing");
        assert_eq!(latest, "1.2.0");
    }

    #[test]
    fn a_matching_tag_leaves_the_version_verdict_alone() {
        let tags = vec!["1.0.0".to_string(), "1.2.0".to_string()];
        let (_, _, status) = pack_row("1.0.0", &tags, &PackTagIntegrity::Matches);
        assert_eq!(status, "outdated");

        let tags = vec!["1.0.0".to_string()];
        let (_, _, status) = pack_row("1.0.0", &tags, &PackTagIntegrity::Matches);
        assert_eq!(status, "up to date");
    }
}
