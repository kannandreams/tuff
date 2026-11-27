//! The install engine the `tuff` command and the pack operations share:
//! one capability rendered into one folder's harnesses and recorded in its
//! lockfile. Not part of the stable library; the pack operations in
//! [`crate::install`] are.
//!
//! Nothing here prints or reads the process environment. Messages go into
//! the caller's [`Messages`], and baseline copies go to the caller's
//! [`Baselines`]. The global scope's lockfile and config paths still come
//! from `tuff_core::paths`, which reads the XDG variables, so library
//! callers use the project scope.

pub mod index;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use tuff_core::adapter::{self, AgentAdapter, CapabilityKind, HookDefinition, HookRenderContext};
use tuff_core::error::{Result, TuffError};
use tuff_core::lockfile::{self, TargetLockEntry};
use tuff_core::manifest::{self, CapabilityType};
use tuff_core::resolver::Scope;

use crate::adapter::AdapterKind;
use crate::build::{contained_planned_path, mcp_entry_tracked};
use crate::workspace::{Baselines, Message, Messages};

/// Where an install goes: a folder, its scope, and where its baseline
/// copies are kept.
#[derive(Debug, Clone, Copy)]
pub struct Destination<'a> {
    pub root: &'a Path,
    pub scope: Scope,
    pub baselines: &'a Baselines,
}

/// How `tuff add` installs a policy.
#[derive(Debug, Clone, Copy, Default)]
pub struct PolicyOptions {
    /// `--accept-unenforced`: install the rules each selected agent
    /// enforces and record the rest, instead of refusing the policy
    /// (RFC-107 D6).
    pub accept_unenforced: bool,
    /// `--runtime-hook`: also register `tuff policy evaluate` for rules a
    /// native setting covers only in part, such as Claude Code command
    /// rules (RFC-107 D3, internal #41).
    pub runtime_hook: bool,
}

/// Take a hook's registrations out of a settings file the adapter no longer
/// writes to, before it registers in the file it reads now.
///
/// The Codex adapter registered in `.agents/hook.json` before 0.12.0 and in
/// `.codex/hooks.json` since, so `tuff update` of a hook installed earlier
/// would otherwise leave the old registration behind where nothing reads
/// it. Registrations in the current file are left to the merge, which
/// replaces them.
fn remove_registrations_at_previous_settings_path(
    install_root: &Path,
    lockfile: &lockfile::Lockfile,
    capability_id: &str,
    adapter: AdapterKind,
    messages: &mut Messages,
) -> Result<()> {
    let Some(entry) = lockfile.capabilities.get(capability_id) else {
        return Ok(());
    };
    let Some(previous) = entry.targets.get(adapter.id()) else {
        return Ok(());
    };
    let mut by_path: BTreeMap<&str, Vec<lockfile::ManagedHook>> = BTreeMap::new();
    for hook in &previous.managed_hooks {
        if hook.settings_path == adapter.hook_settings_relpath() {
            continue;
        }
        // `-a open-agents -a codex` used to record one registration for
        // both targets. The target that still reads that file keeps it.
        let shared = entry.targets.iter().any(|(target_id, target)| {
            target_id != adapter.id()
                && target.managed_hooks.iter().any(|other| {
                    other.settings_path == hook.settings_path
                        && other.event == hook.event
                        && other.command == hook.command
                })
        });
        if shared {
            continue;
        }
        by_path
            .entry(hook.settings_path.as_str())
            .or_default()
            .push(hook.clone());
    }
    for (settings_path, hooks) in by_path {
        if tuff_core::hook_settings::remove_registrations(settings_path, install_root, &hooks)? {
            messages.push(Message::Done(format!(
                "updated {} hook settings -> {settings_path}",
                adapter.display_name()
            )));
        }
    }
    Ok(())
}

/// Take an MCP entry out of a config file the adapter no longer writes to,
/// unless another target of the same capability still records that file.
///
/// The Codex adapter registered in `.agents/mcp.json` before 0.12.0, the
/// file the `open-agents` target still uses, so an entry both recorded is
/// kept for the target that reads it.
fn remove_entry_at_previous_config_path(
    install_root: &Path,
    lockfile: &lockfile::Lockfile,
    capability_id: &str,
    adapter: AdapterKind,
) -> Result<()> {
    let Some(entry) = lockfile.capabilities.get(capability_id) else {
        return Ok(());
    };
    let Some(previous) = entry
        .targets
        .get(adapter.id())
        .and_then(|target| target.managed_mcp_entry.as_ref())
    else {
        return Ok(());
    };
    if previous.config_path == adapter.mcp_config_relpath() {
        return Ok(());
    }
    let shared = entry.targets.iter().any(|(target_id, target)| {
        target_id != adapter.id()
            && target
                .managed_mcp_entry
                .as_ref()
                .is_some_and(|managed| managed.config_path == previous.config_path)
    });
    if shared {
        return Ok(());
    }
    tuff_core::mcp::remove_tool(&install_root.join(&previous.config_path), capability_id)
}

/// Where an installed file landed, as the install report shows it: relative
/// to the project, or under `~/` for the global scope, whose root is the
/// home directory. A bare relative path there reads as the current
/// directory.
fn installed_path_display(path: &Path, install_root: &Path, scope: Scope) -> String {
    let relative = lockfile::relative_or_absolute_fs(path, install_root);
    match scope {
        Scope::Global if path.starts_with(install_root) => format!("~/{relative}"),
        _ => relative,
    }
}

/// Write one planned file to disk and record it as an `EmittedFile`.
///
/// Shared by `install_capability`'s write phase and the generated
/// capability-index skill ([`index`]), which has no hooks or
/// reporting to layer on top — just files to write and hash.
pub fn write_planned_file(
    install_root: &Path,
    planned: &adapter::PlannedFile,
) -> Result<adapter::EmittedFile> {
    let target_path = install_root.join(contained_planned_path(&planned.path)?);
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&target_path, &planned.content)?;

    Ok(adapter::EmittedFile {
        path: planned.path.clone(),
        hash: lockfile::hash_bytes(&planned.content),
        baseline_hash: lockfile::hash_bytes(&planned.content),
    })
}

/// A policy compiled for one agent, ready to write once nothing can refuse.
struct CompiledPolicy {
    /// Each settings file the agent's rules touch, relative to the install
    /// root, with the bytes it should hold. Empty bytes mean a file Tuff
    /// owns and no longer needs.
    files: Vec<(String, Vec<u8>)>,
    managed: Vec<lockfile::ManagedPermission>,
    /// The `tuff policy evaluate` registrations, for rules the agent
    /// enforces through its hook.
    hooks: Vec<lockfile::ManagedHook>,
}

/// Say, per agent, which rules are enforced only partially and why.
fn warn_partially_enforced_policy(
    policy: &tuff_core::policy::PolicyConfig,
    target_ids: &[String],
    messages: &mut Messages,
) -> Result<()> {
    for tid in target_ids {
        let Some(adapter) = AdapterKind::from_id(tid) else {
            continue;
        };
        let gap = |rule: &tuff_core::policy::PolicyRule| adapter.policy_rule_gap(rule);
        for verdict in tuff_core::policy::verdicts(policy, &adapter.policy_compatibility(), &gap)? {
            if verdict.entry.coverage == tuff_hooks_spec::CoverageLevel::Partial {
                let why = verdict
                    .entry
                    .caveat
                    .as_deref()
                    .unwrap_or("partial coverage");
                messages.push(Message::Warning(format!(
                    "{}: rule {} ({}) is enforced partially: {why}",
                    adapter.display_name(),
                    verdict.index + 1,
                    verdict.rule.describe()
                )));
            }
        }
    }
    Ok(())
}

/// The command a harness runs for a policy's `tuff policy evaluate` hook.
pub fn policy_hook_command(adapter: AdapterKind, capability_id: &str) -> String {
    format!(
        "tuff policy evaluate --harness {} --policy {capability_id}",
        adapter.id()
    )
}

/// Compile a policy into each agent's native permission rules and its
/// `tuff policy evaluate` hook, and compute the settings files they will be
/// in, removing rules and registrations a previous install of the same
/// policy recorded and this one no longer has. `runtime_hook`
/// (`--runtime-hook`) also registers the hook for rules a native setting
/// already covers in part; an install that registered it keeps it.
/// Everything here can refuse, a corrupt settings file included, and
/// nothing here writes.
fn compile_policy(
    install_root: &Path,
    capability_id: &str,
    policy: &tuff_core::policy::PolicyConfig,
    adapters: &[AdapterKind],
    lockfile: &lockfile::Lockfile,
    enforcement: &BTreeMap<String, PolicyEnforcement>,
    runtime_hook: bool,
) -> Result<BTreeMap<String, CompiledPolicy>> {
    use tuff_core::policy_eval::PolicyHookUse;
    let mut compiled = BTreeMap::new();
    for adapter in adapters {
        let previous_target = lockfile
            .capabilities
            .get(capability_id)
            .and_then(|entry| entry.targets.get(adapter.id()));
        let previous_hooks = previous_target
            .map(|target| target.managed_hooks.clone())
            .unwrap_or_default();
        let runtime_hook = runtime_hook || !previous_hooks.is_empty();
        let mut managed: Vec<lockfile::ManagedPermission> = Vec::new();
        let mut hook_subjects: Vec<tuff_core::policy::PolicySubjectKind> = Vec::new();
        let enforced = enforcement
            .get(adapter.id())
            .map(|agent| agent.enforced.as_slice())
            .unwrap_or_default();
        for (index, rule) in policy.rules.iter().enumerate() {
            if !enforced.contains(&index) {
                continue;
            }
            let effect = rule.effect()?;
            let hook_use = adapter.policy_hook_use(rule)?;
            if hook_use == PolicyHookUse::Required
                || (hook_use == PolicyHookUse::Optional && runtime_hook)
            {
                let subject = rule.subject()?.kind();
                if !hook_subjects.contains(&subject) {
                    hook_subjects.push(subject);
                }
            }
            if hook_use == PolicyHookUse::Required {
                continue;
            }
            let cannot_compile = || {
                TuffError::unsupported(format!(
                    "{} cannot compile policy rule ({}) natively",
                    adapter.display_name(),
                    rule.describe()
                ))
            };
            let settings_relpath = adapter
                .permission_relpath_for(rule.subject()?.kind())
                .ok_or_else(cannot_compile)?;
            let native = adapter
                .native_permission_rules(rule)?
                .ok_or_else(cannot_compile)?;
            for native_rule in native {
                let permission = lockfile::ManagedPermission {
                    settings_path: settings_relpath.to_string(),
                    list: effect.as_str().to_string(),
                    rule: native_rule,
                };
                if !managed.contains(&permission) {
                    managed.push(permission);
                }
            }
        }
        let as_pairs = |permissions: &mut dyn Iterator<Item = &lockfile::ManagedPermission>| {
            permissions
                .filter_map(|permission| {
                    tuff_core::policy::PolicyEffect::parse(&permission.list)
                        .map(|effect| (effect, permission.rule.clone()))
                })
                .collect::<Vec<_>>()
        };
        let previous = previous_target
            .map(|target| target.managed_permissions.clone())
            .unwrap_or_default();
        let relpaths: std::collections::BTreeSet<&str> = managed
            .iter()
            .chain(&previous)
            .map(|permission| permission.settings_path.as_str())
            .collect();
        let mut files = Vec::new();
        for settings_relpath in relpaths {
            let stale = as_pairs(&mut previous.iter().filter(|permission| {
                permission.settings_path == settings_relpath && !managed.contains(permission)
            }));
            let additions = as_pairs(
                &mut managed
                    .iter()
                    .filter(|permission| permission.settings_path == settings_relpath),
            );
            if stale.is_empty() && additions.is_empty() {
                continue;
            }
            let settings_path = install_root.join(settings_relpath);
            let existing = if settings_path.is_file() {
                Some(fs::read(&settings_path)?)
            } else if additions.is_empty() {
                continue;
            } else {
                None
            };
            let merged = tuff_core::policy::merge_permissions(
                settings_relpath,
                existing.as_deref(),
                &stale,
                &additions,
            )?;
            files.push((settings_relpath.to_string(), merged));
        }
        let hooks = compile_policy_hook(
            install_root,
            capability_id,
            *adapter,
            &hook_subjects,
            &previous_hooks,
            &mut files,
        )?;
        compiled.insert(
            adapter.id().to_string(),
            CompiledPolicy {
                files,
                managed,
                hooks,
            },
        );
    }
    Ok(compiled)
}

/// Register a policy's `tuff policy evaluate` hook for the rules of
/// `subjects`, and take out the registrations `previous` recorded that it
/// no longer makes. The settings file's next bytes go into `files`, merged
/// with any permission rules already compiled into the same file.
fn compile_policy_hook(
    install_root: &Path,
    capability_id: &str,
    adapter: AdapterKind,
    subjects: &[tuff_core::policy::PolicySubjectKind],
    previous: &[lockfile::ManagedHook],
    files: &mut Vec<(String, Vec<u8>)>,
) -> Result<Vec<lockfile::ManagedHook>> {
    let relpath = adapter.hook_settings_relpath();
    let fragment = if subjects.is_empty() {
        None
    } else {
        Some(
            adapter
                .policy_hook_fragment(&policy_hook_command(adapter, capability_id), subjects)
                .ok_or_else(|| {
                    TuffError::unsupported(format!(
                        "{} has no hook to run tuff policy evaluate for policy '{capability_id}'",
                        adapter.display_name()
                    ))
                })?,
        )
    };
    let hooks = match &fragment {
        Some(fragment) => lockfile::managed_hooks_from_fragment(install_root, relpath, fragment)?,
        None => Vec::new(),
    };
    let stale: Vec<lockfile::ManagedHook> = previous
        .iter()
        .filter(|old| {
            !hooks.iter().any(|new| {
                new.settings_path == old.settings_path
                    && new.event == old.event
                    && new.command == old.command
            })
        })
        .cloned()
        .collect();
    if fragment.is_none() && stale.is_empty() {
        return Ok(hooks);
    }
    let position = files.iter().position(|(path, _)| path == relpath);
    let settings_path = install_root.join(relpath);
    let mut bytes = match position {
        Some(index) => Some(files[index].1.clone()),
        None if settings_path.is_file() => Some(fs::read(&settings_path)?),
        None => None,
    };
    if let Some(current) = &bytes
        && !stale.is_empty()
        && let Some(cleaned) =
            tuff_core::hook_settings::without_registrations(relpath, current, &stale)?
    {
        bytes = Some(cleaned);
    }
    if let Some(fragment) = &fragment {
        bytes = Some(adapter.merge_hook_fragment(bytes.as_deref(), fragment)?);
    }
    if let Some(bytes) = bytes {
        match position {
            Some(index) => files[index].1 = bytes,
            None => files.push((relpath.to_string(), bytes)),
        }
    }
    Ok(hooks)
}

/// The rules of a policy one agent enforces, and a record of the rest.
struct PolicyEnforcement {
    /// Zero-based positions of the rules to compile.
    enforced: Vec<usize>,
    unenforced: Vec<lockfile::UnenforcedRule>,
}

/// Check every rule of a policy against every selected agent's matrix,
/// before anything is planned or written.
///
/// A policy reported as installed where nothing enforces it is worse than
/// no policy, because it removes the reason to look. Without `accept`, a
/// rule that any selected agent would not enforce refuses the whole policy,
/// naming each such rule. With `accept` (`--accept-unenforced`, RFC-107 D6),
/// each agent gets the rules it enforces and a record of the rest, and only
/// an agent that enforces none of the rules refuses the policy.
fn policy_enforcement(
    id: &str,
    policy: &tuff_core::policy::PolicyConfig,
    target_ids: &[String],
    accept: bool,
) -> Result<BTreeMap<String, PolicyEnforcement>> {
    let mut by_agent = BTreeMap::new();
    let mut unenforced_lines = Vec::new();
    let mut enforcing_nothing = Vec::new();
    for tid in target_ids {
        let adapter = AdapterKind::from_id(tid).ok_or_else(|| {
            TuffError::usage(format!("unknown harness '{tid}'"))
                .with_hint("run 'tuff harness list' to see available harnesses")
        })?;
        let gap = |rule: &tuff_core::policy::PolicyRule| adapter.policy_rule_gap(rule);
        let (enforced, unenforced) =
            tuff_core::policy::enforcement(policy, &adapter.policy_compatibility(), &gap)?;
        for rule in &unenforced {
            unenforced_lines.push(format!(
                "  {}: rule {} ({}) is not enforced: {}",
                adapter.display_name(),
                rule.rule,
                rule.description,
                rule.reason
            ));
        }
        if enforced.is_empty() {
            enforcing_nothing.push(adapter.display_name().to_string());
        }
        by_agent.insert(
            adapter.id().to_string(),
            PolicyEnforcement {
                enforced,
                unenforced,
            },
        );
    }
    if unenforced_lines.is_empty() {
        return Ok(by_agent);
    }
    if !accept {
        return Err(TuffError::unsupported(format!(
            "policy '{id}' would not be enforced as written, so it was not installed:\n{}",
            unenforced_lines.join("\n")
        ))
        .with_hint(
            "run 'tuff policy matrix' to see what each agent can enforce, or pass --accept-unenforced to install the rules each agent enforces and record the rest in tuff.lock",
        ));
    }
    if !enforcing_nothing.is_empty() {
        let verb = if enforcing_nothing.len() == 1 {
            "enforces"
        } else {
            "enforce"
        };
        return Err(TuffError::unsupported(format!(
            "policy '{id}' was not installed: {} {verb} none of its rules:\n{}",
            enforcing_nothing.join(", "),
            unenforced_lines.join("\n")
        ))
        .with_hint("run 'tuff policy matrix' to see what each agent can enforce"));
    }
    Ok(by_agent)
}

/// Say, per agent, which rules were not installed because the agent does
/// not enforce them.
fn warn_unenforced_policy_rules(
    enforcement: &BTreeMap<String, PolicyEnforcement>,
    messages: &mut Messages,
) {
    for (agent_id, agent) in enforcement {
        let name = AdapterKind::from_id(agent_id)
            .map_or(agent_id.as_str(), |adapter| adapter.display_name());
        for rule in &agent.unenforced {
            messages.push(Message::Warning(format!(
                "{name}: rule {} ({}) is not enforced, so it was not installed: {}; recorded in tuff.lock",
                rule.rule, rule.description, rule.reason
            )));
        }
    }
}

/// Whether an installed policy already recorded rules an agent does not
/// enforce, which only an install with `--accept-unenforced` does. Such a
/// policy keeps accepting them, so `tuff update` needs no flag.
fn policy_recorded_unenforced_rules(install_root: &Path, scope: Scope, id: &str) -> bool {
    lockfile::require_scoped_lockfile(install_root, scope)
        .ok()
        .and_then(|lockfile| lockfile.capabilities.get(id).cloned())
        .is_some_and(|entry| {
            entry
                .targets
                .values()
                .any(|target| !target.unenforced_rules.is_empty())
        })
}

/// Install one capability into `to` for `target_ids`, recording `source`
/// (or the manifest's folder) in the lockfile, and regenerate the
/// capability index.
pub fn install_capability(
    to: Destination<'_>,
    capability: &adapter::ResolvedCapability,
    manifest: &manifest::CapabilityManifest,
    target_ids: &[String],
    source: Option<lockfile::CapabilitySource>,
    policy_options: PolicyOptions,
    messages: &mut Messages,
) -> Result<()> {
    let Destination {
        root: install_root,
        scope,
        baselines,
    } = to;
    for note in manifest::install_notes(manifest) {
        messages.push(Message::Note(note));
    }
    let policy_enforcement = match capability.kind {
        CapabilityKind::Policy { ref policy } => {
            let accept = policy_options.accept_unenforced
                || policy_recorded_unenforced_rules(install_root, scope, &capability.id);
            let enforcement = policy_enforcement(&capability.id, policy, target_ids, accept)?;
            warn_partially_enforced_policy(policy, target_ids, messages)?;
            warn_unenforced_policy_rules(&enforcement, messages);
            enforcement
        }
        _ => BTreeMap::new(),
    };
    let mut adapters = Vec::new();
    for tid in target_ids {
        let adapter = AdapterKind::from_id(tid).ok_or_else(|| {
            TuffError::usage(format!("unknown harness '{}'", tid,))
                .with_hint("run 'tuff harness list' to see available harnesses")
        })?;
        if !adapter.supports(capability.capability_type) {
            return Err(TuffError::unsupported(format!(
                "{} does not yet support {} capabilities",
                adapter.display_name(),
                capability.capability_type
            )));
        }
        if let CapabilityKind::Hook {
            hook: HookDefinition::Command(ref hook_cfg),
        } = capability.kind
        {
            adapter.native_hook_event(&hook_cfg.event)?;
        }
        adapters.push(adapter);
    }

    let mut plans: Vec<(AdapterKind, Vec<adapter::PlannedFile>)> = Vec::new();
    for adapter in &adapters {
        let planned = adapter.plan(capability, install_root)?;
        plans.push((*adapter, planned));
    }

    let lockfile = lockfile::require_scoped_lockfile(install_root, scope)?;
    for (adapter, planned_files) in &plans {
        let is_tracked = lockfile
            .capabilities
            .get(&capability.id)
            .and_then(|e| e.targets.get(adapter.id()))
            .is_some();
        if !is_tracked {
            for f in planned_files {
                if f.allow_existing {
                    continue;
                }
                let target_path = install_root.join(&f.path);
                if target_path.exists() {
                    return Err(TuffError::refused(format!(
                        "refusing to overwrite untracked file at {}",
                        lockfile::relative_or_absolute_fs(&target_path, install_root)
                    ))
                    .with_hint("remove it, or track it in Tuff first"));
                }
            }
        }
    }

    if matches!(
        capability.kind,
        CapabilityKind::Tool { .. } | CapabilityKind::McpServer { .. }
    ) {
        for adapter in &adapters {
            let mcp_path = install_root.join(adapter.mcp_config_relpath());
            tuff_core::mcp::validate_config(&mcp_path)?;

            // For an external server the JSON entry *is* the product, so an
            // entry Tuff never wrote is a collision — refuse here, before a
            // single file lands, rather than after `server.toml` is written.
            if matches!(capability.kind, CapabilityKind::McpServer { .. })
                && !mcp_entry_tracked(&lockfile, &capability.id, adapter.id())
                && tuff_core::mcp::has_server(&mcp_path, &capability.id)?
            {
                return Err(TuffError::refused(format!(
                    "refusing to overwrite untracked MCP server '{}' in {}",
                    capability.id,
                    lockfile::relative_or_absolute_fs(&mcp_path, install_root)
                ))
                .with_hint("remove it by hand, or choose a different capability id"));
            }
        }
    }

    let compiled_policies = match &capability.kind {
        CapabilityKind::Policy { policy } => compile_policy(
            install_root,
            &capability.id,
            policy,
            &adapters,
            &lockfile,
            &policy_enforcement,
            policy_options.runtime_hook,
        )?,
        _ => BTreeMap::new(),
    };

    let mut new_targets: BTreeMap<String, TargetLockEntry> = BTreeMap::new();

    for (adapter, planned_files) in &plans {
        let mut managed_hooks = Vec::new();

        if let CapabilityKind::Hook { hook } = &capability.kind {
            remove_registrations_at_previous_settings_path(
                install_root,
                &lockfile,
                &capability.id,
                *adapter,
                messages,
            )?;
            let hook_root = install_root
                .join(adapter.dir_prefix())
                .join("hooks")
                .join(&capability.id);
            let hook_root_rel = lockfile::relative_or_absolute_fs(&hook_root, install_root);
            match hook {
                HookDefinition::Command(hook_cfg) => {
                    let render = adapter.render_standard_hook(HookRenderContext {
                        capability_id: &capability.id,
                        hook: hook_cfg,
                        source_files: &capability.source_files,
                        repo_root: install_root,
                        track_managed_hooks: true,
                    })?;
                    for diagnostic in render.diagnostics {
                        messages.push(Message::Warning(diagnostic.message));
                    }
                    managed_hooks = render.managed_hooks;
                }
                HookDefinition::Native(native) => {
                    let fragment = adapter::replace_hook_dir_placeholder(
                        native.fragment.clone(),
                        &hook_root_rel,
                    );
                    let settings_path = adapter.hook_settings_relpath();
                    managed_hooks = lockfile::managed_hooks_from_fragment(
                        install_root,
                        settings_path,
                        &fragment,
                    )?;
                }
            }
        }

        for planned in planned_files {
            let target_path = install_root.join(&planned.path);
            write_planned_file(install_root, planned)?;

            if should_print_installed_file(capability, planned) {
                messages.push(Message::Done(format!(
                    "installed {} ({}) -> {}",
                    capability.id,
                    adapter.id(),
                    installed_path_display(&target_path, install_root, scope)
                )));
            }
        }

        if let Some(note) = adapter.install_note(capability.capability_type) {
            messages.push(Message::Note(note.to_string()));
        }

        if let Some(compiled) = compiled_policies.get(adapter.id()) {
            managed_hooks = compiled.hooks.clone();
            for (settings_relpath, merged) in &compiled.files {
                let settings_path = install_root.join(settings_relpath);
                let owned = tuff_core::policy::is_rules_file(settings_relpath)
                    || tuff_core::policy::is_opencode_config(settings_relpath);
                if merged.is_empty() && owned {
                    if settings_path.is_file() {
                        fs::remove_file(&settings_path)?;
                    }
                    continue;
                }
                if let Some(parent) = settings_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&settings_path, merged)?;
                let count = compiled
                    .managed
                    .iter()
                    .filter(|permission| &permission.settings_path == settings_relpath)
                    .count();
                if count > 0 {
                    messages.push(Message::Done(format!(
                        "compiled {count} permission rule(s) for {} ({}) -> {}",
                        capability.id,
                        adapter.id(),
                        lockfile::relative_or_absolute_fs(&settings_path, install_root)
                    )));
                }
                if compiled
                    .hooks
                    .iter()
                    .any(|hook| &hook.settings_path == settings_relpath)
                {
                    messages.push(Message::Done(format!(
                        "registered tuff policy evaluate for {} ({}) -> {}",
                        capability.id,
                        adapter.id(),
                        lockfile::relative_or_absolute_fs(&settings_path, install_root)
                    )));
                }
            }
        }

        let installed_root = install_root
            .join(adapter.dir_prefix())
            .join(capability.capability_type.plural_dir())
            .join(&capability.id);
        let baseline_hash = tuff_core::cache::hash_tree(&installed_root)?;
        baselines.store(install_root, &baseline_hash, &installed_root)?;
        new_targets.insert(
            adapter.id().to_string(),
            TargetLockEntry {
                managed_hooks,
                managed_mcp_entry: None,
                managed_permissions: compiled_policies
                    .get(adapter.id())
                    .map(|compiled| compiled.managed.clone())
                    .unwrap_or_default(),
                unenforced_rules: policy_enforcement
                    .get(adapter.id())
                    .map(|agent| agent.unenforced.clone())
                    .unwrap_or_default(),
                ownership: target_ownership_for(capability, install_root, *adapter),
                sha256: baseline_hash,
                installed_path: lockfile::relative_or_absolute_fs(&installed_root, install_root),
            },
        );
    }

    if let CapabilityKind::Tool {
        implementation: ref impl_cfg,
        ..
    } = capability.kind
    {
        if impl_cfg.mcp {
            for adapter in &adapters {
                let mcp_path = install_root.join(adapter.mcp_config_relpath());

                let mcp_command = impl_cfg.language.clone();
                let entrypoint_path = format!(
                    "{}/tools/{}/{}",
                    adapter.dir_prefix(),
                    capability.id,
                    impl_cfg.entrypoint
                );
                let mcp_args = vec![entrypoint_path];

                tuff_core::mcp::register_tool(&mcp_path, &capability.id, &mcp_command, &mcp_args)?;
                // MCP-native tools get the same per-entry baseline as
                // external servers: the entry shape below must match what
                // `mcp::register_tool` writes.
                let entry_value = serde_json::json!({"command": mcp_command, "args": mcp_args});
                if let Some(target) = new_targets.get_mut(adapter.id()) {
                    target.managed_mcp_entry = Some(lockfile::ManagedMcpEntry {
                        config_path: adapter.mcp_config_relpath().to_string(),
                        baseline_hash: lockfile::managed_mcp_entry_baseline(&entry_value)?,
                    });
                }
                messages.push(Message::Done(format!(
                    "registered MCP server {} ({}) -> {}",
                    capability.id,
                    adapter.id(),
                    lockfile::relative_or_absolute_fs(&mcp_path, install_root)
                )));
            }
        } else {
            for adapter in &adapters {
                let mcp_path = install_root.join(adapter.mcp_config_relpath());
                tuff_core::mcp::remove_tool(&mcp_path, &capability.id)?;
            }
            messages.push(Message::Note(format!(
                "tool '{}' is not MCP-native; copied and tracked without MCP registration",
                capability.id
            )));
        }
    }

    if let CapabilityKind::McpServer { ref server } = capability.kind {
        for adapter in &adapters {
            let mcp_path = install_root.join(adapter.mcp_config_relpath());
            let tracked = mcp_entry_tracked(&lockfile, &capability.id, adapter.id());
            remove_entry_at_previous_config_path(
                install_root,
                &lockfile,
                &capability.id,
                *adapter,
            )?;
            let entry_value = adapter.mcp_server_entry_checked(server)?;
            let baseline = lockfile::managed_mcp_entry_baseline(&entry_value)?;
            tuff_core::mcp::register_server(&mcp_path, &capability.id, entry_value, tracked)?;
            if let Some(target) = new_targets.get_mut(adapter.id()) {
                target.managed_mcp_entry = Some(lockfile::ManagedMcpEntry {
                    config_path: adapter.mcp_config_relpath().to_string(),
                    baseline_hash: baseline,
                });
            }
            messages.push(Message::Done(format!(
                "registered MCP server {} ({}) -> {}",
                capability.id,
                adapter.id(),
                lockfile::relative_or_absolute_fs(&mcp_path, install_root)
            )));
        }
        let required = tuff_core::catalog::required_env(server);
        if !required.is_empty() {
            messages.push(Message::Note(format!(
                "'{}' reads {} from the environment; export {} before starting the harness",
                capability.id,
                if required.len() == 1 {
                    "a variable"
                } else {
                    "variables"
                },
                required.join(", ")
            )));
        }
    }

    let mut lockfile = lockfile;
    let existing_targets = lockfile
        .capabilities
        .get(&capability.id)
        .map(|e| e.targets.clone())
        .unwrap_or_default();

    let mut merged_targets = existing_targets;
    for (k, v) in new_targets {
        merged_targets.insert(k, v);
    }

    let source = source.unwrap_or_else(|| {
        lockfile::CapabilitySource::local(lockfile::relative_or_absolute_fs(
            &manifest.root,
            install_root,
        ))
    });

    lockfile.capabilities.insert(
        capability.id.clone(),
        lockfile::CapabilityLockEntry {
            capability_type: capability.capability_type,
            version: capability.version.clone(),
            version_scheme: source.version_scheme_for(&capability.version),
            description: capability.description.clone(),
            source,
            targets: merged_targets,
            implementation: manifest.implementation.clone(),
            parameters: manifest.parameters.clone(),
            server: manifest.server.clone(),
        },
    );

    lockfile::write_scoped_lockfile(install_root, scope, &lockfile)?;
    index::regenerate_capability_index(to)?;
    Ok(())
}

fn should_print_installed_file(
    capability: &adapter::ResolvedCapability,
    planned: &adapter::PlannedFile,
) -> bool {
    if capability.capability_type != CapabilityType::Skill {
        return true;
    }
    Path::new(&planned.path).file_name() == Some(std::ffi::OsStr::new("SKILL.md"))
}

fn target_ownership_for(
    capability: &adapter::ResolvedCapability,
    install_root: &Path,
    adapter: AdapterKind,
) -> lockfile::TargetOwnership {
    if matches!(
        &capability.kind,
        CapabilityKind::Hook {
            hook: HookDefinition::Native(_)
        }
    ) && is_path_under(
        &capability.source_dir,
        &install_root.join(adapter.dir_prefix()),
    ) {
        lockfile::TargetOwnership::Imported
    } else {
        lockfile::TargetOwnership::Generated
    }
}

fn is_path_under(path: &Path, root: &Path) -> bool {
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    canonical_path.starts_with(canonical_root)
}
