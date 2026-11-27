//! Installing, updating, and removing a pack the way an application does:
//! no `HOME`, no XDG folders, a workspace and a baseline folder the caller
//! names, and nothing written anywhere else.

use std::fs;
use std::path::{Path, PathBuf};

use tuff_core::error::ErrorKind;
use tuff_core::lockfile::{PackArtifactSource, PackSource};
use tuff_packs::build::build_for_declared_targets;
use tuff_packs::install::{
    FileChange, MemberChange, Operation, UpdateRequest, apply, plan_install, plan_remove,
    plan_update,
};
use tuff_packs::workspace::{Baselines, Workspace};

fn example_pack() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/packs/example/review-pack")
}

/// Every file under `root`, relative to it, sorted.
fn files_under(root: &Path) -> Vec<String> {
    fn walk(root: &Path, current: &Path, out: &mut Vec<String>) {
        for entry in fs::read_dir(current).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.push(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let mut out = Vec::new();
    if root.exists() {
        walk(root, root, &mut out);
    }
    out.sort();
    out
}

/// One test, because it changes the process environment.
#[test]
fn packs_install_update_and_remove_without_home() {
    // SAFETY: this is the only test in this binary, so no other thread
    // reads the environment while it changes.
    unsafe {
        for name in [
            "HOME",
            "XDG_CACHE_HOME",
            "XDG_CONFIG_HOME",
            "XDG_STATE_HOME",
            "XDG_DATA_HOME",
        ] {
            std::env::remove_var(name);
        }
    }
    let sandbox = tempfile::tempdir().unwrap();
    // The process runs somewhere unrelated, which must stay empty.
    let elsewhere = sandbox.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    std::env::set_current_dir(&elsewhere).unwrap();
    let baseline_dir = sandbox.path().join("baselines");
    let baselines = Baselines::dir(&baseline_dir);
    let workspace = Workspace::init(sandbox.path().join("ws")).unwrap();
    let root = workspace.root().to_path_buf();
    let lock_before = fs::read(root.join("tuff.lock")).unwrap();

    let built = build_for_declared_targets(&example_pack()).unwrap();
    let artifact = &built.artifact;
    let source = PackSource::Artifact(PackArtifactSource { registry: None });
    let claude = ["claude".to_string()];

    // A plan describes the change and writes nothing.
    let plan = plan_install(&workspace, artifact, &source, &claude).unwrap();
    assert_eq!(plan.operation, Operation::Install);
    assert_eq!(plan.pack, "com.example/review");
    assert_eq!(plan.release.as_ref().unwrap().version, "1.0.0");
    let mut ids = plan
        .members
        .iter()
        .map(|member| (member.id.as_str(), member.change))
        .collect::<Vec<_>>();
    ids.sort_by_key(|(id, _)| *id);
    assert_eq!(
        ids,
        [
            ("github", MemberChange::Add),
            ("lint-hook", MemberChange::Add),
            ("review-skill", MemberChange::Add),
        ]
    );
    let changed = |plan: &tuff_packs::install::Plan, path: &str| {
        plan.files
            .iter()
            .find(|file| file.path == path)
            .map(|file| file.change)
    };
    assert_eq!(changed(&plan, "tuff.lock"), Some(FileChange::Modify));
    assert_eq!(
        changed(&plan, ".claude/skills/review-skill/SKILL.md"),
        Some(FileChange::Create)
    );
    assert!(
        plan.notes.iter().any(|note| note.contains("echo lint")),
        "{:?}",
        plan.notes
    );
    let json = serde_json::to_value(&plan).unwrap();
    assert_eq!(json["operation"], "install");
    assert_eq!(files_under(&root), ["tuff.lock"]);
    assert_eq!(fs::read(root.join("tuff.lock")).unwrap(), lock_before);

    // A plan made before the workspace changed is refused.
    fs::write(root.join("tuff.lock"), b"{}\n").unwrap();
    let error = apply(&workspace, &baselines, plan).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Drift, "{error}");
    fs::write(root.join("tuff.lock"), &lock_before).unwrap();

    // While another change holds the workspace, apply is refused.
    let plan = plan_install(&workspace, artifact, &source, &claude).unwrap();
    let held = workspace.lock().unwrap();
    let error = apply(&workspace, &baselines, plan).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Refused, "{error}");
    drop(held);

    // Applied, the workspace holds exactly the planned files.
    let plan = plan_install(&workspace, artifact, &source, &claude).unwrap();
    let planned = plan
        .files
        .iter()
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    let applied = apply(&workspace, &baselines, plan).unwrap();
    assert_eq!(applied.pack, "com.example/review");
    assert_eq!(files_under(&root), planned);
    assert!(!files_under(&baseline_dir).is_empty());

    // The installed release again: nothing to plan.
    let same = plan_update(
        &workspace,
        UpdateRequest {
            pack: "com.example/review",
            artifact,
            source: &source,
            targets: &[],
            force: false,
        },
    )
    .unwrap();
    assert!(same.is_none());

    // An edited member blocks an update without force.
    let skill = root.join(".claude/skills/review-skill/SKILL.md");
    let original = fs::read(&skill).unwrap();
    fs::write(&skill, b"edited\n").unwrap();
    let error = plan_update(
        &workspace,
        UpdateRequest {
            pack: "com.example/review",
            artifact,
            source: &PackSource::Git(tuff_core::lockfile::PackGitSource {
                url: "https://example.com/review.git".to_string(),
                path: String::new(),
                git_ref: "0".repeat(40),
                tag: Some("v1.0.0".to_string()),
                branch: None,
                format: None,
                plugin: None,
            }),
            targets: &[],
            force: false,
        },
    )
    .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Drift, "{error}");
    fs::write(&skill, original).unwrap();

    // Removing the pack removes everything it installed.
    let plan = plan_remove(&workspace, "com.example/review", false).unwrap();
    assert_eq!(plan.operation, Operation::Remove);
    assert!(
        plan.members
            .iter()
            .all(|member| member.change == MemberChange::Remove)
    );
    assert_eq!(
        changed(&plan, ".claude/skills/review-skill/SKILL.md"),
        Some(FileChange::Delete)
    );
    apply(&workspace, &baselines, plan).unwrap();
    let left = files_under(&root);
    assert!(
        !left.iter().any(|path| path.contains("review-skill")),
        "{left:?}"
    );

    assert_eq!(files_under(&elsewhere), Vec::<String>::new());
}
