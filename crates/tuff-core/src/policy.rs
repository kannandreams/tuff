//! Policy capabilities: rules that narrow what an agent may do.
//!
//! A policy is a list of rules, each with an effect, `deny` or `ask`, and
//! exactly one subject: a command prefix, file paths an agent may not read,
//! file paths it may not edit, or an MCP tool. There is no `allow` effect. A
//! policy can come from anyone's repository or pack, and one that could grant
//! permissions could quietly widen what an agent may do in every project that
//! installs it; a policy that can only take permissions away can at worst be
//! too strict, and too strict is visible.
//!
//! The subjects are deliberately the intersection of what harnesses can
//! match: commands by prefix and paths by glob. A richer rule would compile
//! into something that means less than it says.
//!
//! Every harness declares, per effect and subject, how it enforces such a
//! rule, in the same `full` / `partial` / `unsupported` terms the hooks
//! specification uses. Until a harness compiles policies, every row is
//! `unsupported`, and installing a policy for it is refused rather than
//! reported as installed.

use serde::{Deserialize, Serialize};
use tuff_hooks_spec::CoverageLevel;

use crate::error::{Result, TuffError};
use crate::lockfile::{ManagedPermission, UnenforcedRule};

/// The `[policy]` section of a `type = "policy"` manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PolicyConfig {
    #[serde(default)]
    pub rules: Vec<PolicyRule>,
}

/// What a matching rule does to the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PolicyEffect {
    /// Refuse the call.
    Deny,
    /// Ask a human before the call runs.
    Ask,
}

impl PolicyEffect {
    pub const ALL: [Self; 2] = [Self::Deny, Self::Ask];

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "deny" => Some(Self::Deny),
            "ask" => Some(Self::Ask),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Deny => "deny",
            Self::Ask => "ask",
        }
    }
}

/// The kind of thing a rule matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PolicySubjectKind {
    /// A shell command, matched by a prefix of its arguments.
    Command,
    /// Reading a file, matched by path pattern.
    Read,
    /// Editing or writing a file, matched by path pattern.
    Edit,
    /// Calling an MCP tool, matched by `server:tool` pattern.
    Mcp,
}

impl PolicySubjectKind {
    pub const ALL: [Self; 4] = [Self::Command, Self::Read, Self::Edit, Self::Mcp];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Read => "read",
            Self::Edit => "edit",
            Self::Mcp => "mcp",
        }
    }
}

/// One `[[policy.rules]]` entry, as written.
///
/// `effect` stays a string here so that `effect = "allow"` can be refused
/// with the reason rather than a parser's list of variants.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PolicyRule {
    pub effect: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// A rule's subject, once validated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicySubject<'a> {
    Command(&'a [String]),
    Read(&'a [String]),
    Edit(&'a [String]),
    Mcp { server: &'a str, tool: &'a str },
}

impl PolicySubject<'_> {
    pub const fn kind(&self) -> PolicySubjectKind {
        match self {
            Self::Command(_) => PolicySubjectKind::Command,
            Self::Read(_) => PolicySubjectKind::Read,
            Self::Edit(_) => PolicySubjectKind::Edit,
            Self::Mcp { .. } => PolicySubjectKind::Mcp,
        }
    }
}

impl PolicyRule {
    /// The rule's effect. `allow` is refused with the reason there is none.
    pub fn effect(&self) -> Result<PolicyEffect> {
        match self.effect.as_str() {
            "deny" => Ok(PolicyEffect::Deny),
            "ask" => Ok(PolicyEffect::Ask),
            "allow" => Err(TuffError::refused(
                "a policy rule cannot allow anything: policies only narrow what an agent may do",
            )
            .with_hint(
                "use effect = \"deny\" or \"ask\"; permissions an agent should have belong in the harness's own settings, not in a shareable policy",
            )),
            other => Err(TuffError::usage(format!(
                "policy rule effect must be \"deny\" or \"ask\", not '{}'",
                other.escape_debug()
            ))),
        }
    }

    /// The rule's single subject.
    pub fn subject(&self) -> Result<PolicySubject<'_>> {
        let mut present = Vec::new();
        if self.command.is_some() {
            present.push("command");
        }
        if self.read.is_some() {
            present.push("read");
        }
        if self.edit.is_some() {
            present.push("edit");
        }
        if self.mcp.is_some() {
            present.push("mcp");
        }
        match present.as_slice() {
            [] => {
                return Err(TuffError::usage(
                    "policy rule needs a subject: one of command, read, edit, or mcp",
                ));
            }
            [_] => {}
            several => {
                return Err(TuffError::usage(format!(
                    "policy rule has more than one subject ({}); write one rule per subject",
                    several.join(", ")
                )));
            }
        }

        if let Some(command) = &self.command {
            validate_command(command)?;
            return Ok(PolicySubject::Command(command));
        }
        if let Some(read) = &self.read {
            validate_paths("read", read)?;
            return Ok(PolicySubject::Read(read));
        }
        if let Some(edit) = &self.edit {
            validate_paths("edit", edit)?;
            return Ok(PolicySubject::Edit(edit));
        }
        let mcp = self.mcp.as_deref().expect("one subject is present");
        let (server, tool) = parse_mcp_pattern(mcp)?;
        Ok(PolicySubject::Mcp { server, tool })
    }

    /// A short description for messages, such as `deny command "git push --force"`.
    pub fn describe(&self) -> String {
        let subject = if let Some(command) = &self.command {
            format!("command \"{}\"", command.join(" "))
        } else if let Some(read) = &self.read {
            format!("read {}", quoted_list(read))
        } else if let Some(edit) = &self.edit {
            format!("edit {}", quoted_list(edit))
        } else if let Some(mcp) = &self.mcp {
            format!("mcp \"{mcp}\"")
        } else {
            "no subject".to_string()
        };
        format!("{} {subject}", self.effect)
    }
}

fn quoted_list(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!("\"{item}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

fn validate_command(command: &[String]) -> Result<()> {
    if command.is_empty() {
        return Err(TuffError::usage(
            "policy rule command must name at least the program, such as [\"git\", \"push\"]",
        ));
    }
    for token in command {
        if token.contains('*') {
            return Err(TuffError::usage(format!(
                "policy rule command arguments are literal words, and '*' is not a pattern here: '{}'",
                token.escape_debug()
            ))
            .with_hint("a command rule already matches every command that starts with its arguments"));
        }
        if token.is_empty() || token.chars().any(char::is_whitespace) || token.contains('\0') {
            return Err(TuffError::usage(format!(
                "policy rule command arguments must be single words without spaces: '{}'",
                token.escape_debug()
            ))
            .with_hint("write each argument as its own string: [\"git\", \"push\", \"--force\"]"));
        }
    }
    Ok(())
}

fn validate_paths(subject: &str, patterns: &[String]) -> Result<()> {
    if patterns.is_empty() {
        return Err(TuffError::usage(format!(
            "policy rule {subject} must list at least one path pattern"
        )));
    }
    for pattern in patterns {
        let escapes = pattern.split('/').any(|segment| segment == "..");
        let invalid = pattern.is_empty()
            || pattern.trim() != pattern
            || pattern.starts_with('/')
            || pattern.starts_with('~')
            || pattern.contains(['\\', '\0']);
        if escapes || invalid {
            return Err(TuffError::usage(format!(
                "policy rule {subject} patterns are paths relative to the project root, such as \".env\" or \"secrets/**\": '{}'",
                pattern.escape_debug()
            ))
            .with_hint("a policy governs its project, so patterns cannot start with '/' or '~' or climb out with '..'"));
        }
    }
    Ok(())
}

fn parse_mcp_pattern(pattern: &str) -> Result<(&str, &str)> {
    let invalid = || {
        TuffError::usage(format!(
            "policy rule mcp must be \"server:tool\", where either side may use '*', such as \"github:delete_*\": '{}'",
            pattern.escape_debug()
        ))
    };
    let (server, tool) = pattern.split_once(':').ok_or_else(invalid)?;
    let allowed = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '*'))
    };
    if !allowed(server) || !allowed(tool) {
        return Err(invalid());
    }
    Ok((server, tool))
}

/// Refuse a policy that could not be enforced as written anywhere: no
/// rules, or a rule with no subject, two subjects, an `allow` effect, or a
/// malformed pattern.
pub fn validate_policy(policy: &PolicyConfig) -> Result<()> {
    if policy.rules.is_empty() {
        return Err(TuffError::usage(
            "a policy needs at least one [[policy.rules]] entry",
        ));
    }
    for (index, rule) in policy.rules.iter().enumerate() {
        let context = |error: TuffError| {
            let hint = error.hint().map(str::to_string);
            let rewritten = TuffError::of(
                error.kind(),
                format!("policy rule {}: {}", index + 1, error.message()),
            );
            match hint {
                Some(hint) => rewritten.with_hint(hint),
                None => rewritten,
            }
        };
        rule.effect().map_err(context)?;
        rule.subject().map_err(context)?;
        if let Some(reason) = &rule.reason
            && reason.trim().is_empty()
        {
            return Err(context(TuffError::usage(
                "reason, when given, must not be empty",
            )));
        }
    }
    Ok(())
}

/// How one harness enforces one kind of rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PolicyCoverageEntry {
    pub effect: PolicyEffect,
    pub subject: PolicySubjectKind,
    pub coverage: CoverageLevel,
    /// What the rule compiles to in the harness, when it compiles at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mechanism: Option<String>,
    /// Why coverage is partial or unsupported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caveat: Option<String>,
    /// Where the claim can be checked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// The matrix of a harness Tuff does not compile policies for: every effect
/// and subject `unsupported`, said plainly.
pub fn not_implemented_matrix() -> Vec<PolicyCoverageEntry> {
    PolicyEffect::ALL
        .into_iter()
        .flat_map(|effect| {
            PolicySubjectKind::ALL
                .into_iter()
                .map(move |subject| PolicyCoverageEntry {
                    effect,
                    subject,
                    coverage: CoverageLevel::Unsupported,
                    mechanism: None,
                    caveat: Some(
                        "Tuff does not compile policy rules for this agent yet".to_string(),
                    ),
                    source: None,
                })
        })
        .collect()
}

/// One rule's verdict on one harness.
#[derive(Debug, Clone)]
pub struct RuleVerdict<'a> {
    /// Zero-based position of the rule in the policy.
    pub index: usize,
    pub rule: &'a PolicyRule,
    pub entry: PolicyCoverageEntry,
}

/// Look up every rule in a harness's matrix. A matrix missing a row for a
/// rule's effect and subject is treated as `unsupported`, never as enforced.
pub fn verdicts<'a>(
    policy: &'a PolicyConfig,
    matrix: &[PolicyCoverageEntry],
) -> Result<Vec<RuleVerdict<'a>>> {
    policy
        .rules
        .iter()
        .enumerate()
        .map(|(index, rule)| {
            let effect = rule.effect()?;
            let subject = rule.subject()?.kind();
            let entry = matrix
                .iter()
                .find(|entry| entry.effect == effect && entry.subject == subject)
                .cloned()
                .unwrap_or_else(|| PolicyCoverageEntry {
                    effect,
                    subject,
                    coverage: CoverageLevel::Unsupported,
                    mechanism: None,
                    caveat: Some("this agent declares nothing for this kind of rule".to_string()),
                    source: None,
                });
            Ok(RuleVerdict { index, rule, entry })
        })
        .collect()
}

/// Split a policy's rules by whether a harness enforces them: the zero-based
/// positions of the rules its matrix covers `full` or `partial`, and a
/// record of each rule it covers `unsupported`, for the lockfile when the
/// policy is installed with `--accept-unenforced` (RFC-107 D6).
pub fn enforcement(
    policy: &PolicyConfig,
    matrix: &[PolicyCoverageEntry],
) -> Result<(Vec<usize>, Vec<UnenforcedRule>)> {
    let mut enforced = Vec::new();
    let mut unenforced = Vec::new();
    for verdict in verdicts(policy, matrix)? {
        if verdict.entry.coverage == CoverageLevel::Unsupported {
            unenforced.push(UnenforcedRule {
                rule: verdict.index + 1,
                description: verdict.rule.describe(),
                reason: verdict
                    .entry
                    .caveat
                    .unwrap_or_else(|| "this agent does not enforce this kind of rule".to_string()),
            });
        } else {
            enforced.push(verdict.index);
        }
    }
    Ok((enforced, unenforced))
}

/// Whether a native permissions file is a rules file, one compiled rule per
/// line, such as Codex's `.codex/rules/tuff.rules`, rather than a JSON
/// settings file.
pub fn is_rules_file(relpath: &str) -> bool {
    relpath.ends_with(".rules")
}

/// The first line of a rules file Tuff writes.
pub const RULES_FILE_HEADER: &str = "# Managed by Tuff: rules compiled from policy capabilities. Change the policy and run tuff update rather than editing this file.";

/// `merge_permissions` for a rules file. Tuff owns the file, but lines it
/// did not write are kept. The result is empty when no rule is left, so the
/// caller can remove the file.
fn merge_rules_file(
    relpath: &str,
    existing: Option<&[u8]>,
    remove: &[(PolicyEffect, String)],
    add: &[(PolicyEffect, String)],
) -> Result<Vec<u8>> {
    let text = match existing {
        Some(bytes) => std::str::from_utf8(bytes)
            .map_err(|_| TuffError::corrupt(format!("{relpath} is not valid UTF-8")))?,
        None => "",
    };
    let mut lines: Vec<String> = text
        .lines()
        .filter(|line| !remove.iter().any(|(_, rule)| rule == line))
        .map(str::to_string)
        .collect();
    if !lines.iter().any(|line| line == RULES_FILE_HEADER) {
        lines.insert(0, RULES_FILE_HEADER.to_string());
    }
    for (_, rule) in add {
        if !lines.contains(rule) {
            lines.push(rule.clone());
        }
    }
    let has_rules = lines.iter().any(|line| {
        let line = line.trim();
        !line.is_empty() && !line.starts_with('#')
    });
    if !has_rules {
        return Ok(Vec::new());
    }
    let mut merged = lines.join("\n");
    merged.push('\n');
    Ok(merged.into_bytes())
}

/// Whether a native permissions file is an OpenCode config file, whose
/// `permission` object maps permission names to actions, or to patterns and
/// actions, and whose order OpenCode reads as precedence.
pub fn is_opencode_config(relpath: &str) -> bool {
    std::path::Path::new(relpath)
        .file_name()
        .is_some_and(|name| name == "opencode.json")
}

/// Where `tuff check` reports a compiled rule that is missing: the file for
/// a rules file, the permission for an OpenCode config, and the list for a
/// JSON settings file.
pub fn permission_location(permission: &ManagedPermission) -> String {
    if is_rules_file(&permission.settings_path) {
        permission.settings_path.clone()
    } else if is_opencode_config(&permission.settings_path) {
        let (name, _) = opencode_rule(&permission.rule);
        format!("{}#permission.{name}", permission.settings_path)
    } else {
        format!(
            "{}#permissions.{}",
            permission.settings_path, permission.list
        )
    }
}

/// An OpenCode rule as Tuff records it: the permission name, then a space
/// and a pattern when the rule sits in that permission's object. A rule with
/// no pattern is a top-level `"<name>": "<action>"` entry, as for MCP tools.
fn opencode_rule(rule: &str) -> (&str, Option<&str>) {
    match rule.split_once(' ') {
        Some((name, pattern)) => (name, Some(pattern)),
        None => (rule, None),
    }
}

/// A JSON value that keeps object keys in file order. OpenCode applies the
/// last matching permission rule, so reordering its config changes what it
/// enforces, and serde_json in this workspace sorts keys.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum OrderedJson {
    Object(indexmap::IndexMap<String, OrderedJson>),
    Array(Vec<OrderedJson>),
    Scalar(serde_json::Value),
}

impl OrderedJson {
    fn action(effect: PolicyEffect) -> Self {
        Self::Scalar(serde_json::Value::String(effect.as_str().to_string()))
    }

    fn is_action(&self, effect: PolicyEffect) -> bool {
        matches!(self, Self::Scalar(serde_json::Value::String(action)) if action == effect.as_str())
    }

    /// OpenCode's shorthand `"read": "allow"` means `{"*": "allow"}`.
    fn expand_shorthand(&mut self) {
        if let Self::Scalar(serde_json::Value::String(_)) = self {
            let action = std::mem::replace(self, Self::Object(indexmap::IndexMap::new()));
            if let Self::Object(patterns) = self {
                patterns.insert("*".to_string(), action);
            }
        }
    }
}

pub(crate) const OPENCODE_SCHEMA: &str = "https://opencode.ai/config.json";

/// `merge_permissions` for an OpenCode config file.
///
/// Every key, rule, and position already in the file is kept. Tuff's rules
/// are appended, `ask` before `deny`, so that a `deny` wins where both
/// match. A rule already in the file with the same pattern and a different
/// action is refused rather than overwritten. The result is empty when the
/// file holds nothing but `$schema` after a removal, so the caller can
/// remove it.
fn merge_opencode_config(
    relpath: &str,
    existing: Option<&[u8]>,
    remove: &[(PolicyEffect, String)],
    add: &[(PolicyEffect, String)],
) -> Result<Vec<u8>> {
    let corrupt = |detail: &str| TuffError::corrupt(format!("{relpath} {detail}"));
    let mut root = match existing {
        Some(bytes) if !bytes.iter().all(u8::is_ascii_whitespace) => serde_json::from_slice::<
            OrderedJson,
        >(bytes)
        .map_err(|error| TuffError::corrupt(format!("{relpath} is not valid JSON: {error}")))?,
        _ => OrderedJson::Object(indexmap::IndexMap::from([(
            "$schema".to_string(),
            OrderedJson::Scalar(serde_json::Value::String(OPENCODE_SCHEMA.to_string())),
        )])),
    };
    let OrderedJson::Object(root_map) = &mut root else {
        return Err(corrupt("must be a JSON object"));
    };
    if add.is_empty() && !root_map.contains_key("permission") {
        return render_opencode_config(&root, false);
    }
    let permission = root_map
        .entry("permission".to_string())
        .or_insert_with(|| OrderedJson::Object(indexmap::IndexMap::new()));
    permission.expand_shorthand();
    let OrderedJson::Object(permission) = permission else {
        return Err(corrupt("field 'permission' must be an object"));
    };

    for (effect, rule) in remove {
        let (name, pattern) = opencode_rule(rule);
        let Some(pattern) = pattern else {
            if permission
                .get(name)
                .is_some_and(|value| value.is_action(*effect))
            {
                permission.shift_remove(name);
            }
            continue;
        };
        let emptied = match permission.get_mut(name) {
            Some(OrderedJson::Object(patterns))
                if patterns
                    .get(pattern)
                    .is_some_and(|value| value.is_action(*effect)) =>
            {
                patterns.shift_remove(pattern);
                patterns.is_empty()
            }
            _ => false,
        };
        if emptied {
            permission.shift_remove(name);
        }
    }

    let conflict = |name: &str, pattern: Option<&str>| {
        let rule = match pattern {
            Some(pattern) => format!("permission.{name} \"{pattern}\""),
            None => format!("permission \"{name}\""),
        };
        TuffError::refused(format!(
            "{relpath} already has its own {rule} with a different action, so the policy was not installed"
        ))
        .with_hint("remove or change that rule in the file, or change the policy")
    };
    let ordered = add
        .iter()
        .filter(|(effect, _)| *effect == PolicyEffect::Ask)
        .chain(
            add.iter()
                .filter(|(effect, _)| *effect == PolicyEffect::Deny),
        );
    for (effect, rule) in ordered {
        let (name, pattern) = opencode_rule(rule);
        let action = OrderedJson::action(*effect);
        match pattern {
            None => {
                if permission
                    .get(name)
                    .is_some_and(|value| !value.is_action(*effect))
                {
                    return Err(conflict(name, None));
                }
                permission.shift_remove(name);
                permission.insert(name.to_string(), action);
            }
            Some(pattern) => {
                let entry = permission
                    .entry(name.to_string())
                    .or_insert_with(|| OrderedJson::Object(indexmap::IndexMap::new()));
                entry.expand_shorthand();
                let OrderedJson::Object(patterns) = entry else {
                    return Err(corrupt(&format!(
                        "field 'permission.{name}' must be an object or an action"
                    )));
                };
                if patterns
                    .get(pattern)
                    .is_some_and(|value| !value.is_action(*effect))
                {
                    return Err(conflict(name, Some(pattern)));
                }
                patterns.shift_remove(pattern);
                patterns.insert(pattern.to_string(), action);
            }
        }
    }

    if !remove.is_empty() && permission.is_empty() {
        root_map.shift_remove("permission");
    }
    let only_schema = root_map.keys().all(|key| key == "$schema");
    render_opencode_config(&root, !remove.is_empty() && only_schema)
}

fn render_opencode_config(root: &OrderedJson, empty: bool) -> Result<Vec<u8>> {
    if empty {
        return Ok(Vec::new());
    }
    let mut text = serde_json::to_string_pretty(root)?;
    text.push('\n');
    Ok(text.into_bytes())
}

/// Add and remove native permission rules in a harness settings file, given
/// the bytes it holds now, and return the bytes it should hold next.
///
/// The file belongs to the user, as with hook registrations: every other key
/// and every rule Tuff did not write is kept. A rule already present is not
/// added twice. A `deny` or `ask` list that this call empties is removed, and
/// so is a `permissions` object this call leaves empty. A file that is not
/// JSON, or whose `permissions` or a touched list has the wrong type, is
/// refused as corrupt, so a caller can run this before writing anything.
///
/// A rules file (`is_rules_file`) holds one rule per line instead, and comes
/// back empty when no rule is left in it.
pub fn merge_permissions(
    settings_relpath: &str,
    existing: Option<&[u8]>,
    remove: &[(PolicyEffect, String)],
    add: &[(PolicyEffect, String)],
) -> Result<Vec<u8>> {
    if is_rules_file(settings_relpath) {
        return merge_rules_file(settings_relpath, existing, remove, add);
    }
    if is_opencode_config(settings_relpath) {
        return merge_opencode_config(settings_relpath, existing, remove, add);
    }
    let mut settings: serde_json::Value = match existing {
        Some(bytes) if !bytes.is_empty() => serde_json::from_slice(bytes).map_err(|error| {
            TuffError::corrupt(format!("{settings_relpath} is not valid JSON: {error}"))
        })?,
        _ => serde_json::json!({}),
    };
    let object = settings
        .as_object_mut()
        .ok_or_else(|| TuffError::corrupt(format!("{settings_relpath} must be a JSON object")))?;
    if add.is_empty() && !object.contains_key("permissions") {
        return Ok(serde_json::to_string_pretty(&settings)?.into_bytes());
    }
    let permissions = object
        .entry("permissions")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| {
            TuffError::corrupt(format!(
                "{settings_relpath} field 'permissions' must be an object"
            ))
        })?;
    let not_a_list = |effect: PolicyEffect| {
        TuffError::corrupt(format!(
            "{settings_relpath} field 'permissions.{}' must be an array",
            effect.as_str()
        ))
    };
    for (effect, rule) in remove {
        if let Some(list) = permissions.get_mut(effect.as_str()) {
            let list = list.as_array_mut().ok_or_else(|| not_a_list(*effect))?;
            list.retain(|entry| entry.as_str() != Some(rule.as_str()));
        }
    }
    for (effect, rule) in add {
        let list = permissions
            .entry(effect.as_str())
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
            .ok_or_else(|| not_a_list(*effect))?;
        if !list
            .iter()
            .any(|entry| entry.as_str() == Some(rule.as_str()))
        {
            list.push(serde_json::Value::String(rule.clone()));
        }
    }
    for effect in PolicyEffect::ALL {
        let emptied_here = remove.iter().any(|(removed, _)| *removed == effect)
            && permissions
                .get(effect.as_str())
                .and_then(serde_json::Value::as_array)
                .is_some_and(Vec::is_empty);
        if emptied_here {
            permissions.remove(effect.as_str());
        }
    }
    let now_empty = !remove.is_empty() && permissions.is_empty();
    if now_empty {
        object.remove("permissions");
    }
    Ok(serde_json::to_string_pretty(&settings)?.into_bytes())
}

/// Take recorded permission rules back out of their settings files.
///
/// A settings file that no longer exists holds nothing to remove. One that
/// is not valid JSON stops the removal, before the caller deletes anything.
pub fn remove_permissions(
    repo_root: &std::path::Path,
    managed: &[ManagedPermission],
) -> Result<()> {
    let mut by_file: std::collections::BTreeMap<&str, Vec<(PolicyEffect, String)>> =
        std::collections::BTreeMap::new();
    for permission in managed {
        if let Some(effect) = PolicyEffect::parse(&permission.list) {
            by_file
                .entry(permission.settings_path.as_str())
                .or_default()
                .push((effect, permission.rule.clone()));
        }
    }
    for (relpath, removals) in by_file {
        let path = repo_root.join(relpath);
        if !path.is_file() {
            continue;
        }
        let bytes = std::fs::read(&path)?;
        let mut merged = merge_permissions(relpath, Some(&bytes), &removals, &[])?;
        // A rules file and an OpenCode config both come back empty once
        // nothing Tuff or the user wrote is left in them.
        if is_rules_file(relpath) || is_opencode_config(relpath) {
            // The rules file exists only to hold compiled rules.
            if merged.is_empty() {
                std::fs::remove_file(&path)?;
            } else if merged != bytes {
                std::fs::write(&path, merged)?;
            }
            continue;
        }
        if merged != bytes {
            merged.push(b'\n');
            std::fs::write(&path, merged)?;
        }
    }
    Ok(())
}

/// Whether a recorded rule is still in its list: `clean` when it is,
/// `missing` when the rule or the file is gone, `modified` when the file is
/// no longer valid JSON.
pub fn managed_permission_status(
    repo_root: &std::path::Path,
    permission: &ManagedPermission,
) -> &'static str {
    let Ok(raw) = std::fs::read_to_string(repo_root.join(&permission.settings_path)) else {
        return "missing";
    };
    if is_opencode_config(&permission.settings_path) {
        let Ok(settings) = serde_json::from_str::<serde_json::Value>(&raw) else {
            return "modified";
        };
        let (name, pattern) = opencode_rule(&permission.rule);
        let entry = settings
            .get("permission")
            .and_then(|permissions| permissions.get(name));
        let action = match pattern {
            Some(pattern) => entry.and_then(|patterns| patterns.get(pattern)),
            None => entry,
        };
        return if action.and_then(serde_json::Value::as_str) == Some(permission.list.as_str()) {
            "clean"
        } else {
            "missing"
        };
    }
    if is_rules_file(&permission.settings_path) {
        return if raw.lines().any(|line| line == permission.rule) {
            "clean"
        } else {
            "missing"
        };
    }
    let Ok(settings) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return "modified";
    };
    let present = settings
        .get("permissions")
        .and_then(|permissions| permissions.get(&permission.list))
        .and_then(serde_json::Value::as_array)
        .is_some_and(|list| {
            list.iter()
                .any(|entry| entry.as_str() == Some(permission.rule.as_str()))
        });
    if present { "clean" } else { "missing" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    fn deny(rule: &str) -> (PolicyEffect, String) {
        (PolicyEffect::Deny, rule.to_string())
    }

    fn ask(rule: &str) -> (PolicyEffect, String) {
        (PolicyEffect::Ask, rule.to_string())
    }

    #[test]
    fn merging_permissions_keeps_the_users_rules_and_adds_each_rule_once() {
        let existing = br#"{"model": "opus", "permissions": {"deny": ["Bash(curl *)"], "allow": ["Bash(npm test *)"]}}"#;
        let add = [
            deny("Bash(git push --force *)"),
            ask("Bash(terraform apply *)"),
        ];
        let once = merge_permissions(".claude/settings.json", Some(existing), &[], &add).unwrap();
        let twice = merge_permissions(".claude/settings.json", Some(&once), &[], &add).unwrap();
        assert_eq!(once, twice, "a redundant merge leaves the file unchanged");
        let settings: serde_json::Value = serde_json::from_slice(&once).unwrap();
        assert_eq!(settings["model"], "opus");
        assert_eq!(
            settings["permissions"]["deny"],
            serde_json::json!(["Bash(curl *)", "Bash(git push --force *)"])
        );
        assert_eq!(
            settings["permissions"]["ask"],
            serde_json::json!(["Bash(terraform apply *)"])
        );
        assert_eq!(
            settings["permissions"]["allow"],
            serde_json::json!(["Bash(npm test *)"])
        );
    }

    #[test]
    fn removing_permissions_prunes_only_what_it_emptied() {
        let existing = br#"{"permissions": {"deny": ["Bash(curl *)", "Bash(git push --force *)"], "ask": ["Bash(terraform apply *)"]}}"#;
        let merged = merge_permissions(
            "s.json",
            Some(existing),
            &[
                deny("Bash(git push --force *)"),
                ask("Bash(terraform apply *)"),
            ],
            &[],
        )
        .unwrap();
        let settings: serde_json::Value = serde_json::from_slice(&merged).unwrap();
        assert_eq!(
            settings,
            serde_json::json!({"permissions": {"deny": ["Bash(curl *)"]}})
        );

        let only_ours =
            br#"{"model": "opus", "permissions": {"ask": ["Bash(terraform apply *)"]}}"#;
        let merged = merge_permissions(
            "s.json",
            Some(only_ours),
            &[ask("Bash(terraform apply *)")],
            &[],
        )
        .unwrap();
        let settings: serde_json::Value = serde_json::from_slice(&merged).unwrap();
        assert_eq!(settings, serde_json::json!({"model": "opus"}));

        let untouched = br#"{"permissions": {}}"#;
        let merged = merge_permissions("s.json", Some(untouched), &[], &[]).unwrap();
        let settings: serde_json::Value = serde_json::from_slice(&merged).unwrap();
        assert_eq!(
            settings,
            serde_json::json!({"permissions": {}}),
            "nothing removed, nothing pruned"
        );
    }

    #[test]
    fn a_corrupt_settings_file_is_refused() {
        for (bytes, expected) in [
            (&b"{ not json"[..], "is not valid JSON"),
            (&b"[]"[..], "must be a JSON object"),
            (
                &br#"{"permissions": []}"#[..],
                "'permissions' must be an object",
            ),
            (
                &br#"{"permissions": {"deny": "x"}}"#[..],
                "'permissions.deny' must be an array",
            ),
        ] {
            let error = merge_permissions(
                ".claude/settings.json",
                Some(bytes),
                &[],
                &[deny("Bash(rm *)")],
            )
            .unwrap_err();
            assert_eq!(error.kind(), ErrorKind::Corrupt, "{error}");
            assert!(error.to_string().contains(expected), "{error}");
        }
    }

    #[test]
    fn recorded_permission_status_and_removal_from_disk() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join(".claude")).unwrap();
        let path = temp.path().join(".claude/settings.json");
        std::fs::write(
            &path,
            r#"{"permissions": {"deny": ["Bash(curl *)", "Bash(rm *)"]}}"#,
        )
        .unwrap();
        let ours = ManagedPermission {
            settings_path: ".claude/settings.json".to_string(),
            list: "deny".to_string(),
            rule: "Bash(rm *)".to_string(),
        };
        assert_eq!(managed_permission_status(temp.path(), &ours), "clean");
        remove_permissions(temp.path(), std::slice::from_ref(&ours)).unwrap();
        assert_eq!(managed_permission_status(temp.path(), &ours), "missing");
        let settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            settings,
            serde_json::json!({"permissions": {"deny": ["Bash(curl *)"]}})
        );

        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(managed_permission_status(temp.path(), &ours), "modified");
        assert!(remove_permissions(temp.path(), &[ours]).is_err());
    }

    #[test]
    fn a_rules_file_holds_one_rule_per_line_and_is_removed_when_emptied() {
        const RELPATH: &str = ".codex/rules/tuff.rules";
        let forbid =
            deny(r#"prefix_rule(pattern = ["git", "push", "--force"], decision = "forbidden")"#);
        let prompt = ask(r#"prefix_rule(pattern = ["terraform", "apply"], decision = "prompt")"#);
        let once =
            merge_permissions(RELPATH, None, &[], &[forbid.clone(), prompt.clone()]).unwrap();
        let twice =
            merge_permissions(RELPATH, Some(&once), &[], std::slice::from_ref(&forbid)).unwrap();
        assert_eq!(once, twice, "a redundant merge leaves the file unchanged");
        assert_eq!(
            String::from_utf8(once.clone()).unwrap(),
            format!("{RULES_FILE_HEADER}\n{}\n{}\n", forbid.1, prompt.1)
        );

        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join(".codex/rules")).unwrap();
        let path = temp.path().join(RELPATH);
        std::fs::write(&path, &once).unwrap();
        let recorded = |(effect, rule): &(PolicyEffect, String)| ManagedPermission {
            settings_path: RELPATH.to_string(),
            list: effect.as_str().to_string(),
            rule: rule.clone(),
        };
        assert_eq!(
            managed_permission_status(temp.path(), &recorded(&forbid)),
            "clean"
        );
        remove_permissions(temp.path(), &[recorded(&forbid)]).unwrap();
        assert_eq!(
            managed_permission_status(temp.path(), &recorded(&forbid)),
            "missing"
        );
        assert_eq!(
            managed_permission_status(temp.path(), &recorded(&prompt)),
            "clean"
        );
        remove_permissions(temp.path(), &[recorded(&prompt)]).unwrap();
        assert!(!path.exists(), "a rules file with no rules left is removed");
    }

    #[test]
    fn an_opencode_config_keeps_the_users_order_and_puts_policy_rules_last() {
        const RELPATH: &str = ".opencode/opencode.json";
        let existing = br#"{"$schema": "https://opencode.ai/config.json", "permission": {"bash": {"*": "allow", "git push *": "allow"}, "read": "allow"}, "model": "x"}"#;
        let add = [
            deny("bash git push --force *"),
            ask("bash terraform apply *"),
            deny("read .env"),
            deny("read */.env"),
            deny("github_delete_*"),
        ];
        let once = merge_permissions(RELPATH, Some(existing), &[], &add).unwrap();
        let twice = merge_permissions(RELPATH, Some(&once), &[], &add).unwrap();
        assert_eq!(once, twice, "a redundant merge leaves the file unchanged");
        let OrderedJson::Object(root) = serde_json::from_slice::<OrderedJson>(&once).unwrap()
        else {
            panic!("an object")
        };
        assert_eq!(
            root.keys().collect::<Vec<_>>(),
            ["$schema", "permission", "model"]
        );
        let OrderedJson::Object(permission) = &root["permission"] else {
            panic!("an object")
        };
        assert_eq!(
            permission.keys().collect::<Vec<_>>(),
            ["bash", "read", "github_delete_*"]
        );
        let OrderedJson::Object(bash) = &permission["bash"] else {
            panic!("an object")
        };
        assert_eq!(
            bash.keys().collect::<Vec<_>>(),
            ["*", "git push *", "terraform apply *", "git push --force *"],
            "ask rules come before deny rules, both after the user's"
        );
        let OrderedJson::Object(read) = &permission["read"] else {
            panic!("an object")
        };
        assert_eq!(read.keys().collect::<Vec<_>>(), ["*", ".env", "*/.env"]);

        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join(".opencode")).unwrap();
        std::fs::write(temp.path().join(RELPATH), &once).unwrap();
        let recorded: Vec<ManagedPermission> = add
            .iter()
            .map(|(effect, rule)| ManagedPermission {
                settings_path: RELPATH.to_string(),
                list: effect.as_str().to_string(),
                rule: rule.clone(),
            })
            .collect();
        for permission in &recorded {
            assert_eq!(
                managed_permission_status(temp.path(), permission),
                "clean",
                "{permission:?}"
            );
        }
        assert_eq!(
            permission_location(&recorded[0]),
            ".opencode/opencode.json#permission.bash"
        );
        remove_permissions(temp.path(), &recorded).unwrap();
        let left: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(temp.path().join(RELPATH)).unwrap())
                .unwrap();
        assert_eq!(
            left,
            serde_json::json!({
                "$schema": "https://opencode.ai/config.json",
                "permission": {"bash": {"*": "allow", "git push *": "allow"}, "read": {"*": "allow"}},
                "model": "x"
            })
        );
    }

    #[test]
    fn an_opencode_config_refuses_a_conflicting_rule_and_empties_when_only_tuff_wrote_it() {
        const RELPATH: &str = ".opencode/opencode.json";
        let conflicting = br#"{"permission": {"bash": {"git push --force *": "allow"}}}"#;
        let error = merge_permissions(
            RELPATH,
            Some(conflicting),
            &[],
            &[deny("bash git push --force *")],
        )
        .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Refused, "{error}");

        let rule = deny("bash git push --force *");
        let created = merge_permissions(RELPATH, None, &[], std::slice::from_ref(&rule)).unwrap();
        let removed =
            merge_permissions(RELPATH, Some(&created), std::slice::from_ref(&rule), &[]).unwrap();
        assert!(
            removed.is_empty(),
            "a file with only $schema left is removed"
        );

        let error = merge_permissions(RELPATH, Some(b"[]"), &[], &[rule]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Corrupt, "{error}");
    }

    #[test]
    fn a_command_argument_cannot_be_a_pattern() {
        let policy =
            parse("[[policy.rules]]\neffect = \"deny\"\ncommand = [\"git\", \"push\", \"*\"]\n");
        let error = validate_policy(&policy).unwrap_err();
        assert!(
            error.to_string().contains("'*' is not a pattern here"),
            "{error}"
        );
    }

    fn parse(toml_body: &str) -> PolicyConfig {
        #[derive(Deserialize)]
        struct Wrapper {
            policy: PolicyConfig,
        }
        toml::from_str::<Wrapper>(toml_body)
            .expect("valid TOML")
            .policy
    }

    const INFRA: &str = r#"
[[policy.rules]]
effect = "deny"
command = ["git", "push", "--force"]
reason = "Force pushes rewrite shared history."

[[policy.rules]]
effect = "deny"
read = [".env", "secrets/**"]

[[policy.rules]]
effect = "ask"
command = ["terraform", "apply"]

[[policy.rules]]
effect = "deny"
mcp = "github:delete_*"
"#;

    #[test]
    fn the_infrastructure_example_is_a_valid_policy() {
        let policy = parse(INFRA);
        validate_policy(&policy).unwrap();
        let kinds: Vec<_> = policy
            .rules
            .iter()
            .map(|rule| (rule.effect().unwrap(), rule.subject().unwrap().kind()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                (PolicyEffect::Deny, PolicySubjectKind::Command),
                (PolicyEffect::Deny, PolicySubjectKind::Read),
                (PolicyEffect::Ask, PolicySubjectKind::Command),
                (PolicyEffect::Deny, PolicySubjectKind::Mcp),
            ]
        );
        assert_eq!(
            policy.rules[0].describe(),
            "deny command \"git push --force\""
        );
        assert_eq!(
            policy.rules[1].describe(),
            "deny read \".env\", \"secrets/**\""
        );
    }

    #[test]
    fn a_policy_cannot_allow_anything() {
        let policy = parse("[[policy.rules]]\neffect = \"allow\"\ncommand = [\"rm\"]\n");
        let error = validate_policy(&policy).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Refused);
        assert!(error.to_string().contains("policy rule 1"), "{error}");
        assert!(error.to_string().contains("only narrow"), "{error}");
    }

    #[test]
    fn each_rule_has_exactly_one_subject() {
        let none = parse("[[policy.rules]]\neffect = \"deny\"\n");
        assert!(
            validate_policy(&none)
                .unwrap_err()
                .to_string()
                .contains("needs a subject")
        );
        let two =
            parse("[[policy.rules]]\neffect = \"deny\"\ncommand = [\"rm\"]\nread = [\".env\"]\n");
        assert!(
            validate_policy(&two)
                .unwrap_err()
                .to_string()
                .contains("more than one subject (command, read)")
        );
    }

    #[test]
    fn malformed_rules_are_refused_with_the_rule_number() {
        for (body, expected) in [
            (
                "effect = \"block\"\ncommand = [\"rm\"]",
                "must be \"deny\" or \"ask\"",
            ),
            ("effect = \"deny\"\ncommand = []", "at least the program"),
            (
                "effect = \"deny\"\ncommand = [\"git push\"]",
                "without spaces",
            ),
            ("effect = \"deny\"\nread = []", "at least one path pattern"),
            (
                "effect = \"deny\"\nread = [\"../outside\"]",
                "relative to the project root",
            ),
            (
                "effect = \"deny\"\nedit = [\"/etc/passwd\"]",
                "relative to the project root",
            ),
            (
                "effect = \"deny\"\nread = [\"~/.ssh/id_rsa\"]",
                "relative to the project root",
            ),
            ("effect = \"deny\"\nmcp = \"github\"", "\"server:tool\""),
            ("effect = \"deny\"\nmcp = \"git hub:x\"", "\"server:tool\""),
            ("effect = \"deny\"\nmcp = \"github:\"", "\"server:tool\""),
            (
                "effect = \"deny\"\ncommand = [\"rm\"]\nreason = \" \"",
                "must not be empty",
            ),
        ] {
            let policy = parse(&format!(
                "[[policy.rules]]\neffect = \"deny\"\ncommand = [\"ok\"]\n\n[[policy.rules]]\n{body}\n"
            ));
            let error = validate_policy(&policy).unwrap_err();
            let text = error.to_string();
            assert!(text.contains("policy rule 2"), "{body}: {text}");
            assert!(text.contains(expected), "{body}: {text}");
        }
    }

    #[test]
    fn an_empty_policy_is_refused() {
        let error = validate_policy(&PolicyConfig { rules: Vec::new() }).unwrap_err();
        assert!(error.to_string().contains("at least one"), "{error}");
    }

    #[test]
    fn unknown_keys_in_a_rule_are_a_parse_error() {
        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct Wrapper {
            policy: PolicyConfig,
        }
        let result =
            toml::from_str::<Wrapper>("[[policy.rules]]\neffect = \"deny\"\npath = [\".env\"]\n");
        assert!(result.is_err(), "a misspelt subject must not be ignored");
    }

    #[test]
    fn the_not_implemented_matrix_covers_every_effect_and_subject_as_unsupported() {
        let matrix = not_implemented_matrix();
        assert_eq!(
            matrix.len(),
            PolicyEffect::ALL.len() * PolicySubjectKind::ALL.len()
        );
        assert!(
            matrix
                .iter()
                .all(|entry| entry.coverage == CoverageLevel::Unsupported && entry.caveat.is_some())
        );
    }

    #[test]
    fn enforcement_separates_the_rules_a_harness_enforces_from_the_ones_it_does_not() {
        let policy = parse(INFRA);
        let mut matrix = Vec::new();
        for effect in PolicyEffect::ALL {
            for subject in [PolicySubjectKind::Command, PolicySubjectKind::Mcp] {
                matrix.push(PolicyCoverageEntry {
                    effect,
                    subject,
                    coverage: CoverageLevel::Partial,
                    mechanism: Some("native".to_string()),
                    caveat: None,
                    source: None,
                });
            }
        }
        let (enforced, unenforced) = enforcement(&policy, &matrix).unwrap();
        assert_eq!(enforced, vec![0, 2, 3]);
        assert_eq!(
            unenforced,
            vec![UnenforcedRule {
                rule: 2,
                description: "deny read \".env\", \"secrets/**\"".to_string(),
                reason: "this agent declares nothing for this kind of rule".to_string(),
            }]
        );

        let (enforced, unenforced) = enforcement(&policy, &not_implemented_matrix()).unwrap();
        assert!(enforced.is_empty());
        assert_eq!(unenforced.len(), 4);
        assert_eq!(
            unenforced[0].reason,
            "Tuff does not compile policy rules for this agent yet"
        );
    }

    #[test]
    fn a_rule_the_matrix_does_not_mention_is_never_treated_as_enforced() {
        let policy = parse(INFRA);
        let matrix = vec![PolicyCoverageEntry {
            effect: PolicyEffect::Deny,
            subject: PolicySubjectKind::Command,
            coverage: CoverageLevel::Partial,
            mechanism: Some("native".to_string()),
            caveat: None,
            source: None,
        }];
        let verdicts = verdicts(&policy, &matrix).unwrap();
        let coverage: Vec<_> = verdicts
            .iter()
            .map(|verdict| verdict.entry.coverage)
            .collect();
        assert_eq!(
            coverage,
            vec![
                CoverageLevel::Partial,
                CoverageLevel::Unsupported,
                CoverageLevel::Unsupported,
                CoverageLevel::Unsupported,
            ]
        );
    }
}
