//! Hook registrations inside a harness's settings file.
//!
//! This is the one piece of adapter logic that is genuinely per-harness,
//! and until it lived here every adapter crate carried its own copy of it.
//! The copies drifted: a dedupe bug had to be fixed four times at once
//! (tuff#83), and each crate spelled the same validation error a little
//! differently. What actually differs between harnesses is the *shape* of
//! the file, captured by [`HookSettingsShape`]; the merge and the removal
//! are the same algorithm over either shape.

use std::path::Path;

use crate::error::{Result, TuffError};
use crate::lockfile::{self, ManagedHook};

/// How a harness lays out the hook registrations in its settings file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookSettingsShape {
    /// Claude Code's shape, shared by Open Agents and Codex: every event
    /// holds groups, and every group holds typed hook entries.
    ///
    /// ```json
    /// {"hooks": {"PreToolUse": [{"hooks": [{"type": "command", "command": "sh …"}]}]}}
    /// ```
    Grouped,
    /// Cursor's shape: every event holds the entries directly, and the
    /// file carries a `version` beside `hooks`.
    ///
    /// ```json
    /// {"version": 1, "hooks": {"preToolUse": [{"command": "sh …"}]}}
    /// ```
    Flat,
}

/// The `version` a flat settings file declares.
const FLAT_SETTINGS_VERSION: u64 = 1;

impl HookSettingsShape {
    /// The settings a harness starts from when the file does not exist.
    fn empty_settings(self) -> serde_json::Value {
        match self {
            Self::Grouped => serde_json::json!({}),
            Self::Flat => serde_json::json!({"version": FLAT_SETTINGS_VERSION}),
        }
    }

    /// The keys a hooks-only fragment may carry beside `hooks`.
    fn extra_fragment_keys(self) -> &'static [&'static str] {
        match self {
            Self::Grouped => &[],
            Self::Flat => &["version"],
        }
    }

    /// The fragment that registers one command under one native event.
    pub fn command_fragment(self, native_event: &str, command: &str) -> serde_json::Value {
        match self {
            Self::Grouped => serde_json::json!({
                "hooks": {
                    native_event: [{
                        "hooks": [{"type": "command", "command": command}]
                    }]
                }
            }),
            Self::Flat => serde_json::json!({
                "version": FLAT_SETTINGS_VERSION,
                "hooks": {native_event: [{"command": command}]}
            }),
        }
    }

    /// Refuse anything that is not a hooks-only fragment. A whole settings
    /// file pasted in by mistake would otherwise be merged key by key into
    /// the harness's real one.
    pub fn validate_fragment(self, fragment: &serde_json::Value) -> Result<()> {
        let object = fragment
            .as_object()
            .ok_or_else(|| TuffError::usage("--hook-file fragment must be a JSON object"))?;
        if !object.contains_key("hooks") {
            return Err(TuffError::usage(
                "--hook-file fragment must contain a top-level 'hooks' object",
            ));
        }
        let allowed = self.extra_fragment_keys();
        if object
            .keys()
            .any(|key| key != "hooks" && !allowed.contains(&key.as_str()))
        {
            return Err(TuffError::usage(match self {
                Self::Grouped => {
                    "--hook-file must be a hooks-only fragment, not a full settings file"
                }
                Self::Flat => "--hook-file must contain only 'hooks' and optional 'version'",
            }));
        }
        if !fragment["hooks"].is_object() {
            return Err(TuffError::usage(
                "--hook-file field 'hooks' must be an object",
            ));
        }
        Ok(())
    }

    /// Merge a hooks-only fragment into the harness's settings file, given
    /// the bytes it holds now, and return the bytes it should hold next.
    ///
    /// Everything the user already has is kept. `tuff add` is re-runnable
    /// and a pack may be installed over an existing install, so a group the
    /// event already registers is not appended again; the harness would
    /// otherwise run that hook once per copy.
    pub fn merge_fragment(
        self,
        settings_relpath: &str,
        existing: Option<&[u8]>,
        fragment: &serde_json::Value,
    ) -> Result<Vec<u8>> {
        self.validate_fragment(fragment)?;
        let mut settings = match existing {
            Some(bytes) if !bytes.is_empty() => serde_json::from_slice(bytes)?,
            _ => self.empty_settings(),
        };
        let settings_object = settings.as_object_mut().ok_or_else(|| {
            TuffError::corrupt(format!("{settings_relpath} must be a JSON object"))
        })?;
        if self == Self::Flat {
            settings_object
                .entry("version")
                .or_insert_with(|| serde_json::json!(FLAT_SETTINGS_VERSION));
        }
        let fragment_hooks = fragment["hooks"]
            .as_object()
            .expect("validated hooks object");
        let settings_hooks = settings_object
            .entry("hooks")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| {
                TuffError::corrupt(format!(
                    "{settings_relpath} field 'hooks' must be an object"
                ))
            })?;
        for (event, additions) in fragment_hooks {
            let additions = additions.as_array().ok_or_else(|| {
                TuffError::usage(format!("--hook-file hooks.{event} must be an array"))
            })?;
            let groups = settings_hooks
                .entry(event.clone())
                .or_insert_with(|| serde_json::json!([]))
                .as_array_mut()
                .ok_or_else(|| {
                    TuffError::corrupt(format!("{settings_relpath} hooks.{event} must be an array"))
                })?;
            extend_hook_groups(groups, additions);
        }
        Ok(serde_json::to_string_pretty(&settings)?.into_bytes())
    }
}

/// Append hook groups that this event does not already register.
pub fn extend_hook_groups(existing: &mut Vec<serde_json::Value>, additions: &[serde_json::Value]) {
    for addition in additions {
        if !existing.iter().any(|group| group == addition) {
            existing.push(addition.clone());
        }
    }
}

/// Take Tuff's registrations back out of the harness's settings file,
/// leaving everything else in it alone.
///
/// The registrations are matched by command, the way the lockfile records
/// them, so this needs no shape: a grouped entry is found inside its
/// group's `hooks`, a flat entry is the group itself, and a group or an
/// event left empty is pruned so the file reads as if Tuff had never
/// written to it.
pub fn remove_registrations(
    settings_relpath: &str,
    display_name: &str,
    repo_root: &Path,
    managed_hooks: &[ManagedHook],
) -> Result<()> {
    if managed_hooks.is_empty() {
        return Ok(());
    }
    let settings_path = repo_root.join(settings_relpath);
    if !settings_path.is_file() {
        return Ok(());
    }
    let mut settings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path)?)?;
    let Some(hooks) = settings
        .get_mut("hooks")
        .and_then(|hooks| hooks.as_object_mut())
    else {
        return Ok(());
    };
    let mut empty_events = Vec::new();
    for (event, groups) in hooks.iter_mut() {
        let Some(groups) = groups.as_array_mut() else {
            continue;
        };
        let registered: Vec<&str> = managed_hooks
            .iter()
            .filter(|hook| hook.settings_path == settings_relpath && hook.event == *event)
            .map(|hook| hook.command.as_str())
            .collect();
        let is_registered = |entry: &serde_json::Value| {
            entry
                .get("command")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|command| registered.contains(&command))
        };
        for group in groups.iter_mut() {
            if let Some(entries) = group
                .get_mut("hooks")
                .and_then(|value| value.as_array_mut())
            {
                entries.retain(|entry| !is_registered(entry));
            }
        }
        groups.retain(
            |group| match group.get("hooks").and_then(|value| value.as_array()) {
                Some(entries) => !entries.is_empty(),
                None => !is_registered(group),
            },
        );
        if groups.is_empty() {
            empty_events.push(event.clone());
        }
    }
    for event in empty_events {
        hooks.remove(&event);
    }
    std::fs::write(
        &settings_path,
        serde_json::to_string_pretty(&settings)? + "\n",
    )?;
    eprintln!(
        "updated {display_name} hook settings -> {}",
        lockfile::relative_or_absolute_fs(&settings_path, repo_root)
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn managed(settings_path: &str, event: &str, command: &str) -> ManagedHook {
        ManagedHook {
            settings_path: settings_path.to_string(),
            event: event.to_string(),
            canonical_event: None,
            command: command.to_string(),
            baseline_hash: String::new(),
        }
    }

    #[test]
    fn a_grouped_fragment_registers_a_typed_entry_inside_a_group() {
        let fragment = HookSettingsShape::Grouped.command_fragment("PreToolUse", "sh run.sh");
        assert_eq!(
            fragment,
            serde_json::json!({
                "hooks": {"PreToolUse": [{"hooks": [{"type": "command", "command": "sh run.sh"}]}]}
            })
        );
    }

    #[test]
    fn a_flat_fragment_registers_the_entry_directly_and_declares_a_version() {
        let fragment = HookSettingsShape::Flat.command_fragment("preToolUse", "sh run.sh");
        assert_eq!(
            fragment,
            serde_json::json!({
                "version": 1,
                "hooks": {"preToolUse": [{"command": "sh run.sh"}]}
            })
        );
    }

    #[test]
    fn merging_the_same_fragment_twice_does_not_duplicate_the_hook() {
        // tuff#83: the harness ran a hook once per copy. Pinned for both
        // shapes, since the fix once had to be applied in four places.
        for shape in [HookSettingsShape::Grouped, HookSettingsShape::Flat] {
            let fragment = shape.command_fragment("before_finish", "sh .agents/hooks/x/run.sh");
            let once = shape
                .merge_fragment("settings.json", None, &fragment)
                .expect("first merge");
            let twice = shape
                .merge_fragment("settings.json", Some(&once), &fragment)
                .expect("second merge");
            let settings: serde_json::Value = serde_json::from_slice(&twice).unwrap();
            assert_eq!(
                settings["hooks"]["before_finish"].as_array().unwrap().len(),
                1,
                "{shape:?}: re-adding a hook must not register it twice"
            );
            assert_eq!(
                once, twice,
                "{shape:?}: a redundant merge must leave the file unchanged"
            );
        }
    }

    #[test]
    fn merging_keeps_what_the_user_already_had() {
        let existing = br#"{"permissions": {"allow": ["Bash"]}, "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "theirs"}]}]}}"#;
        let fragment = HookSettingsShape::Grouped.command_fragment("Stop", "ours");
        let merged = HookSettingsShape::Grouped
            .merge_fragment(".claude/settings.json", Some(existing), &fragment)
            .unwrap();
        let settings: serde_json::Value = serde_json::from_slice(&merged).unwrap();
        assert_eq!(settings["permissions"]["allow"][0], "Bash");
        let groups = settings["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0]["hooks"][0]["command"], "theirs");
        assert_eq!(groups[1]["hooks"][0]["command"], "ours");
    }

    #[test]
    fn a_flat_file_without_a_version_gains_one() {
        let existing = br#"{"hooks": {}}"#;
        let fragment = HookSettingsShape::Flat.command_fragment("stop", "sh run.sh");
        let merged = HookSettingsShape::Flat
            .merge_fragment(".cursor/hooks.json", Some(existing), &fragment)
            .unwrap();
        let settings: serde_json::Value = serde_json::from_slice(&merged).unwrap();
        assert_eq!(settings["version"], 1);
    }

    #[test]
    fn a_whole_settings_file_is_refused_as_a_fragment() {
        let full = serde_json::json!({"permissions": {}, "hooks": {}});
        let error = HookSettingsShape::Grouped
            .validate_fragment(&full)
            .unwrap_err();
        assert!(error.to_string().contains("hooks-only"), "{error}");

        // The flat shape carries its version beside the hooks, and only that.
        HookSettingsShape::Flat
            .validate_fragment(&serde_json::json!({"version": 1, "hooks": {}}))
            .unwrap();
        let error = HookSettingsShape::Flat
            .validate_fragment(&full)
            .unwrap_err();
        assert!(error.to_string().contains("optional 'version'"), "{error}");

        for shape in [HookSettingsShape::Grouped, HookSettingsShape::Flat] {
            assert!(shape.validate_fragment(&serde_json::json!([])).is_err());
            assert!(shape.validate_fragment(&serde_json::json!({})).is_err());
            assert!(
                shape
                    .validate_fragment(&serde_json::json!({"hooks": []}))
                    .is_err()
            );
        }
    }

    #[test]
    fn a_settings_file_that_is_not_an_object_is_reported_as_corrupt() {
        let error = HookSettingsShape::Grouped
            .merge_fragment(
                ".claude/settings.json",
                Some(b"[]"),
                &HookSettingsShape::Grouped.command_fragment("Stop", "x"),
            )
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains(".claude/settings.json must be a JSON object"),
            "{error}"
        );
    }

    #[test]
    fn removal_takes_out_only_tuff_registrations_in_either_shape() {
        let temp = tempfile::tempdir().unwrap();
        let grouped = r#"{"permissions": {}, "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "theirs"}, {"type": "command", "command": "ours"}]}], "PreToolUse": [{"hooks": [{"type": "command", "command": "ours"}]}]}}"#;
        std::fs::create_dir_all(temp.path().join(".claude")).unwrap();
        std::fs::write(temp.path().join(".claude/settings.json"), grouped).unwrap();
        remove_registrations(
            ".claude/settings.json",
            "Claude",
            temp.path(),
            &[
                managed(".claude/settings.json", "Stop", "ours"),
                managed(".claude/settings.json", "PreToolUse", "ours"),
                managed(".cursor/hooks.json", "stop", "theirs"),
            ],
        )
        .unwrap();
        let settings: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(temp.path().join(".claude/settings.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            settings,
            serde_json::json!({
                "permissions": {},
                "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "theirs"}]}]}
            }),
            "the event Tuff emptied is pruned; the other file's registration is ignored"
        );

        let flat = r#"{"version": 1, "hooks": {"stop": [{"command": "theirs"}, {"command": "ours"}], "preToolUse": [{"command": "ours"}]}}"#;
        std::fs::create_dir_all(temp.path().join(".cursor")).unwrap();
        std::fs::write(temp.path().join(".cursor/hooks.json"), flat).unwrap();
        remove_registrations(
            ".cursor/hooks.json",
            "Cursor",
            temp.path(),
            &[
                managed(".cursor/hooks.json", "stop", "ours"),
                managed(".cursor/hooks.json", "preToolUse", "ours"),
            ],
        )
        .unwrap();
        let settings: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(temp.path().join(".cursor/hooks.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            settings,
            serde_json::json!({"version": 1, "hooks": {"stop": [{"command": "theirs"}]}})
        );
    }

    #[test]
    fn removal_with_nothing_registered_or_no_file_is_a_no_op() {
        let temp = tempfile::tempdir().unwrap();
        remove_registrations(".claude/settings.json", "Claude", temp.path(), &[]).unwrap();
        remove_registrations(
            ".claude/settings.json",
            "Claude",
            temp.path(),
            &[managed(".claude/settings.json", "Stop", "ours")],
        )
        .unwrap();
        assert!(!temp.path().join(".claude/settings.json").exists());
    }
}
