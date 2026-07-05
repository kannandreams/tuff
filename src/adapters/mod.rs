pub mod claude_code;
pub mod codex;

use std::path::Path;

pub(crate) fn relative_path(path: &Path, repo_root: &Path) -> String {
    path.strip_prefix(repo_root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().to_string())
}
