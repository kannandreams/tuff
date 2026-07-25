use std::path::{Path, PathBuf};

pub fn project_config(root: &Path) -> PathBuf {
    root.join("tuff.config.json")
}

pub fn user_config(home: &Path) -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"))
        .join("tuff")
        .join("config.json")
}

pub fn user_cache(home: &Path) -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".cache"))
        .join("tuff")
}

pub fn user_state(home: &Path) -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local").join("state"))
        .join("tuff")
}

pub fn global_lockfile(home: &Path) -> PathBuf {
    user_state(home).join("tuff.lock")
}
