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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

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
