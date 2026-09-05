//! Releases of a git-sourced capability, read from the repository's tags
//! (RFC-101).
//!
//! Nobody assigns versions to skills today, so Tuff derives them from the
//! one convention that already exists in the wild: a tag such as `v1.4.0`.
//! A monorepo tags per capability, `<name>/v1.4.0` or `<name>-v1.4.0`, and
//! when any tag is scoped to the capability only those count, so a repo-wide
//! `v2.0.0` cannot be mistaken for a release of one of its skills.
//!
//! Everything here is pure: the caller lists the tags (`git ls-remote`) and
//! clones the chosen one. The lockfile keeps pinning the commit; the tag and
//! the requirement are recorded beside it so `update` can move within the
//! requirement and `outdated` can say how far behind an install is.

use std::fmt;

use semver::{Version, VersionReq};

use crate::error::{Result, TuffError};

/// What the user asked for after the `@` in `name@1.2.0` or `name@^1.2`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionRequest {
    /// Exactly this release.
    Exact(Version),
    /// The highest release inside a range.
    Range(VersionReq),
}

impl VersionRequest {
    /// Parse the text after `@`. An exact version is tried first, because
    /// `1.2.0` also parses as the range `^1.2.0` and the user who typed a
    /// full version meant that version. A leading `v` is accepted on an
    /// exact version since that is how the tag is usually spelled.
    pub fn parse(text: &str) -> Result<Self> {
        let text = text.trim();
        if text.is_empty() {
            return Err(TuffError::usage("a version requirement cannot be empty"));
        }
        if let Ok(version) = Version::parse(text.strip_prefix('v').unwrap_or(text)) {
            return Ok(Self::Exact(version));
        }
        VersionReq::parse(text).map(Self::Range).map_err(|error| {
            TuffError::usage(format!(
                "'{text}' is not a version or a version range: {error}"
            ))
            .with_hint("use an exact release such as 1.2.0, or a range such as ^1.2 or >=1, <2")
        })
    }

    pub fn matches(&self, version: &Version) -> bool {
        match self {
            Self::Exact(exact) => exact == version,
            Self::Range(range) => range.matches(version),
        }
    }
}

impl fmt::Display for VersionRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exact(version) => write!(f, "{version}"),
            Self::Range(range) => write!(f, "{range}"),
        }
    }
}

/// Split `name@requirement` into its parts. A bare `name` has no
/// requirement; an empty name or an empty requirement is a usage error.
pub fn split_version_request(spec: &str) -> Result<(&str, Option<&str>)> {
    let Some((name, request)) = spec.split_once('@') else {
        return Ok((spec, None));
    };
    if name.is_empty() {
        return Err(TuffError::usage(format!(
            "'{spec}' has a version requirement but no capability name"
        ))
        .with_hint("write <name>@<version>, as in security-review@^1.2"));
    }
    if request.is_empty() {
        return Err(
            TuffError::usage(format!("'{spec}' ends in '@' with no version after it"))
                .with_hint("write <name>@<version>, as in security-review@^1.2, or drop the '@'"),
        );
    }
    Ok((name, Some(request)))
}

/// A tag that names a release of one capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseTag {
    /// The tag as the repository spells it, `v1.4.0` or `foo/v1.4.0`.
    pub tag: String,
    pub version: Version,
    /// Whether the tag names the capability, as in a monorepo.
    pub scoped: bool,
}

/// Read one tag as a release of `name`, if it is one. Recognised shapes are
/// `v1.4.0` and `1.4.0` for the whole repository, and `<name>/v1.4.0` or
/// `<name>-v1.4.0`, with or without the `v`, for one capability in it.
pub fn parse_release_tag(tag: &str, name: &str) -> Option<ReleaseTag> {
    let scoped_rest = if name.is_empty() {
        None
    } else {
        tag.strip_prefix(name)
            .and_then(|rest| rest.strip_prefix('/').or_else(|| rest.strip_prefix('-')))
    };
    let (rest, scoped) = match scoped_rest {
        Some(rest) => (rest, true),
        None => (tag, false),
    };
    let version = Version::parse(rest.strip_prefix('v').unwrap_or(rest)).ok()?;
    Some(ReleaseTag {
        tag: tag.to_string(),
        version,
        scoped,
    })
}

/// The releases of `name` among a repository's tags, oldest first. When any
/// tag is scoped to the capability, only scoped tags count: a monorepo's
/// repo-wide tag is not a release of one of its members.
pub fn release_tags<'a>(tags: impl IntoIterator<Item = &'a str>, name: &str) -> Vec<ReleaseTag> {
    let mut releases: Vec<ReleaseTag> = tags
        .into_iter()
        .filter_map(|tag| parse_release_tag(tag, name))
        .collect();
    if releases.iter().any(|release| release.scoped) {
        releases.retain(|release| release.scoped);
    }
    releases.sort_by(|a, b| a.version.cmp(&b.version));
    releases
}

/// The newest release, if there is one.
pub fn latest_release(releases: &[ReleaseTag]) -> Option<&ReleaseTag> {
    releases.iter().max_by(|a, b| a.version.cmp(&b.version))
}

/// The newest release satisfying the request.
pub fn select_release<'a>(
    releases: &'a [ReleaseTag],
    request: &VersionRequest,
) -> Option<&'a ReleaseTag> {
    releases
        .iter()
        .filter(|release| request.matches(&release.version))
        .max_by(|a, b| a.version.cmp(&b.version))
}

/// Choose the release of `name` that satisfies `request`, from a raw tag
/// list, with an error that says what was available when nothing does.
pub fn resolve_release(
    tags: &[String],
    name: &str,
    request: &VersionRequest,
) -> Result<ReleaseTag> {
    let releases = release_tags(tags.iter().map(String::as_str), name);
    if releases.is_empty() {
        return Err(TuffError::not_found(format!(
            "no release tags for '{name}' in the repository"
        ))
        .with_hint(format!(
            "a tag such as v1.2.0 or {name}/v1.2.0 marks a release; omit the version to install the latest commit"
        )));
    }
    match select_release(&releases, request) {
        Some(release) => Ok(release.clone()),
        None => {
            let available: Vec<String> = releases
                .iter()
                .rev()
                .take(10)
                .map(|release| release.version.to_string())
                .collect();
            Err(TuffError::not_found(format!(
                "no release of '{name}' matches {request}; available: {}",
                available.join(", ")
            )))
        }
    }
}

/// The claimed size of moving from one release to another, read from the
/// version numbers alone. It is what the author claimed, not what the diff
/// shows, which is why `outdated` prints it beside the versions and
/// `tuff diff --upstream` stays one keystroke away.
pub fn change_kind(from: &Version, to: &Version) -> &'static str {
    if to.major != from.major {
        "major"
    } else if to.minor != from.minor {
        "minor"
    } else {
        "patch"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(list: &[&str]) -> Vec<String> {
        list.iter().map(|tag| tag.to_string()).collect()
    }

    #[test]
    fn a_full_version_is_exact_and_a_partial_one_is_a_range() {
        assert_eq!(
            VersionRequest::parse("1.2.0").unwrap(),
            VersionRequest::Exact(Version::new(1, 2, 0))
        );
        assert_eq!(
            VersionRequest::parse("v1.2.0").unwrap(),
            VersionRequest::Exact(Version::new(1, 2, 0))
        );
        let range = VersionRequest::parse("^1.2").unwrap();
        assert!(range.matches(&Version::new(1, 9, 0)));
        assert!(!range.matches(&Version::new(2, 0, 0)));
        let bare_major = VersionRequest::parse("1").unwrap();
        assert!(bare_major.matches(&Version::new(1, 4, 0)));
        assert!(!bare_major.matches(&Version::new(2, 0, 0)));
    }

    #[test]
    fn an_unparsable_requirement_is_a_usage_error() {
        let error = VersionRequest::parse("latest").unwrap_err();
        assert_eq!(error.exit_code(), 2, "{error}");
        assert!(VersionRequest::parse("").is_err());
    }

    #[test]
    fn name_and_requirement_split_at_the_at_sign() {
        assert_eq!(split_version_request("foo").unwrap(), ("foo", None));
        assert_eq!(
            split_version_request("foo@^1.2").unwrap(),
            ("foo", Some("^1.2"))
        );
        assert!(split_version_request("@1").is_err());
        assert!(split_version_request("foo@").is_err());
    }

    #[test]
    fn repo_wide_and_scoped_tag_shapes_are_recognised() {
        for (tag, scoped) in [
            ("v1.4.0", false),
            ("1.4.0", false),
            ("foo/v1.4.0", true),
            ("foo-v1.4.0", true),
            ("foo/1.4.0", true),
            ("foo-1.4.0", true),
        ] {
            let release = parse_release_tag(tag, "foo").unwrap_or_else(|| panic!("{tag}"));
            assert_eq!(release.version, Version::new(1, 4, 0), "{tag}");
            assert_eq!(release.scoped, scoped, "{tag}");
            assert_eq!(release.tag, tag);
        }
        for tag in [
            "release-42",
            "foobar/v1.0.0",
            "bar/v1.0.0",
            "foo-bar-v1.0.0",
            "v1",
        ] {
            assert!(parse_release_tag(tag, "foo").is_none(), "{tag}");
        }
    }

    #[test]
    fn scoped_tags_hide_repo_wide_ones_in_a_monorepo() {
        // The repository is at 9.9.9; the skill inside it is at 1.0.0. The
        // repo-wide tag must not read as a release of the skill.
        let releases = release_tags(["v9.9.9", "foo/v1.0.0", "bar/v3.0.0"], "foo");
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].tag, "foo/v1.0.0");

        let releases = release_tags(["v1.0.0", "v1.2.0", "nightly"], "foo");
        assert_eq!(
            releases.len(),
            2,
            "with no scoped tag, repo-wide tags count"
        );
    }

    #[test]
    fn selection_takes_the_highest_match_by_version_not_by_string() {
        let releases = release_tags(["v1.9.0", "v1.10.0", "v2.0.0", "v1.2.0"], "foo");
        let caret = VersionRequest::parse("^1").unwrap();
        assert_eq!(select_release(&releases, &caret).unwrap().tag, "v1.10.0");
        let exact = VersionRequest::parse("1.2.0").unwrap();
        assert_eq!(select_release(&releases, &exact).unwrap().tag, "v1.2.0");
        assert_eq!(latest_release(&releases).unwrap().tag, "v2.0.0");
        let three = VersionRequest::parse("^3").unwrap();
        assert!(select_release(&releases, &three).is_none());
    }

    #[test]
    fn prereleases_are_not_picked_by_a_range() {
        let releases = release_tags(["v1.0.0", "v2.0.0-rc.1"], "foo");
        let any = VersionRequest::parse(">=1").unwrap();
        assert_eq!(select_release(&releases, &any).unwrap().tag, "v1.0.0");
        let exact = VersionRequest::parse("2.0.0-rc.1").unwrap();
        assert_eq!(
            select_release(&releases, &exact).unwrap().tag,
            "v2.0.0-rc.1"
        );
    }

    #[test]
    fn resolve_explains_no_tags_and_no_match_differently() {
        let request = VersionRequest::parse("^2").unwrap();
        let none = resolve_release(&tags(&["nightly"]), "foo", &request).unwrap_err();
        assert!(
            none.to_string().contains("no release tags for 'foo'"),
            "{none}"
        );
        assert!(
            none.hint().is_some_and(|hint| hint.contains("foo/v1.2.0")),
            "{none:?}"
        );

        let miss = resolve_release(&tags(&["v1.2.0", "v1.4.0"]), "foo", &request).unwrap_err();
        assert!(
            miss.to_string().contains("no release of 'foo' matches ^2"),
            "{miss}"
        );
        assert!(
            miss.to_string().contains("available: 1.4.0, 1.2.0"),
            "{miss}"
        );

        let hit = resolve_release(
            &tags(&["v1.2.0", "v1.4.0"]),
            "foo",
            &VersionRequest::parse("^1").unwrap(),
        )
        .unwrap();
        assert_eq!(hit.tag, "v1.4.0");
    }

    #[test]
    fn change_kind_reads_the_version_delta() {
        let v = |text: &str| Version::parse(text).unwrap();
        assert_eq!(change_kind(&v("1.2.0"), &v("1.4.0")), "minor");
        assert_eq!(change_kind(&v("1.2.0"), &v("2.0.0")), "major");
        assert_eq!(change_kind(&v("1.2.0"), &v("1.2.3")), "patch");
    }
}
