//! `tuff policy evaluate`: matching one tool call against a policy at run
//! time (RFC-107 D3).
//!
//! Where a harness has no native setting for a rule, it can run a hook
//! before each tool call and let the hook answer. The harness adapter turns
//! its hook input into [`PolicyAction`]s and the decision back into the
//! harness's answer; the matching lives here, once.
//!
//! A shell command is read the way a shell would split it, then reduced to
//! the programs it runs: a path becomes the program name
//! (`/usr/bin/git` is `git`), wrappers such as `env`, `sudo`, and
//! `sh -c "..."` are unwrapped, command substitutions are read as commands
//! of their own, and options before a subcommand are skipped
//! (`git -C . push`). The files a command names on its command line, as
//! arguments of programs such as `cat` or as redirections, are checked
//! against `read` and `edit` rules. A script or program that runs a
//! command or opens a file itself is not seen, which is why coverage
//! through the hook stays `partial`.

use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::error::{Result, TuffError};
use crate::policy::{PolicyConfig, PolicyEffect, PolicyRule, PolicySubject};

/// How deep wrappers and substitutions are unwrapped. A command nested
/// deeper than this is still checked at the depth reached.
const MAX_DEPTH: usize = 8;

/// One thing a tool call is about to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyAction {
    /// A shell command line, as the agent wrote it.
    Shell(String),
    /// Reading a file, by path as the harness gave it.
    Read(String),
    /// Writing, editing, or deleting a file.
    Edit(String),
    /// Calling a tool of an MCP server.
    Mcp { server: String, tool: String },
}

/// A tool call, as a harness adapter reads it from the hook input.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PolicyHookRequest {
    /// The native event the harness ran the hook for.
    pub event: String,
    /// The working directory of the call, when the input says.
    pub cwd: Option<PathBuf>,
    /// Workspace roots the harness names, when it does.
    pub roots: Vec<PathBuf>,
    pub actions: Vec<PolicyAction>,
}

/// What the hook prints and the status it exits with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyHookAnswer {
    pub stdout: String,
    pub exit_code: i32,
}

/// How a harness uses the `tuff policy evaluate` hook for one rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyHookUse {
    /// The rule compiles to a native setting only.
    Never,
    /// The hook is how the harness enforces the rule.
    Required,
    /// A native setting enforces the rule, and the hook, registered with
    /// `--runtime-hook`, also catches forms of it the setting misses.
    Optional,
}

/// What `tuff policy evaluate` concluded about one call.
#[derive(Debug, Clone, Copy)]
pub enum PolicyVerdict<'a> {
    /// No rule matched; the harness decides as it would without the hook.
    NoMatch,
    Matched(&'a PolicyDecision),
    /// The input or the policy could not be read; the call is refused.
    Failed(&'a str),
}

/// The rule a call matched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PolicyDecision {
    pub effect: PolicyEffect,
    pub policy: String,
    /// One-based position of the rule in the policy.
    pub rule: usize,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl PolicyDecision {
    /// The message shown to the agent and the user.
    pub fn message(&self) -> String {
        let verb = match self.effect {
            PolicyEffect::Deny => "denies",
            PolicyEffect::Ask => "asks before",
        };
        let mut message = format!(
            "Tuff policy '{}' {verb} this call: rule {} ({})",
            self.policy, self.rule, self.description
        );
        if let Some(reason) = &self.reason {
            message.push_str(": ");
            message.push_str(reason);
        }
        message
    }
}

/// Where a call happens: the project the policy governs and the directory
/// relative paths in the call start from.
#[derive(Debug, Clone, Copy)]
pub struct EvalContext<'a> {
    pub root: &'a Path,
    pub cwd: &'a Path,
}

/// The strongest rule of `policy` that any of `actions` matches: a deny
/// over an ask, and the first rule of that effect in the policy.
pub fn evaluate(
    policy_id: &str,
    policy: &PolicyConfig,
    actions: &[PolicyAction],
    context: EvalContext<'_>,
) -> Result<Option<PolicyDecision>> {
    let facts = Facts::gather(actions, context);
    let mut best: Option<PolicyDecision> = None;
    for (index, rule) in policy.rules.iter().enumerate() {
        let effect = rule.effect()?;
        if best
            .as_ref()
            .is_some_and(|best| best.effect == PolicyEffect::Deny || effect == PolicyEffect::Ask)
        {
            continue;
        }
        if facts.matches(rule)? {
            best = Some(PolicyDecision {
                effect,
                policy: policy_id.to_string(),
                rule: index + 1,
                description: rule.describe(),
                reason: rule.reason.clone(),
            });
        }
    }
    Ok(best)
}

/// Everything a set of actions does, reduced to the terms rules match.
#[derive(Debug, Default)]
struct Facts {
    /// Every program invocation, program name first.
    commands: Vec<Vec<String>>,
    /// Project-relative paths read.
    reads: Vec<String>,
    /// Project-relative paths written, edited, or deleted.
    edits: Vec<String>,
    mcp: Vec<(String, String)>,
}

impl Facts {
    fn gather(actions: &[PolicyAction], context: EvalContext<'_>) -> Self {
        let mut facts = Self::default();
        let mut reads = Vec::new();
        let mut edits = Vec::new();
        for action in actions {
            match action {
                PolicyAction::Shell(script) => {
                    for command in shell_commands(script) {
                        let (command_reads, command_edits) = command_files(&command);
                        reads.extend(command_reads);
                        edits.extend(command_edits);
                        if !command.argv.is_empty() {
                            facts.commands.push(command.argv);
                        }
                    }
                }
                PolicyAction::Read(path) => reads.push(path.clone()),
                PolicyAction::Edit(path) => edits.push(path.clone()),
                PolicyAction::Mcp { server, tool } => {
                    facts.mcp.push((server.clone(), tool.clone()));
                }
            }
        }
        facts.reads = reads
            .iter()
            .filter_map(|path| project_relative(path, context))
            .collect();
        facts.edits = edits
            .iter()
            .filter_map(|path| project_relative(path, context))
            .collect();
        facts
    }

    fn matches(&self, rule: &PolicyRule) -> Result<bool> {
        Ok(match rule.subject()? {
            PolicySubject::Command(words) => self
                .commands
                .iter()
                .any(|argv| command_matches(words, argv)),
            PolicySubject::Read(patterns) => self
                .reads
                .iter()
                .any(|path| patterns.iter().any(|pattern| path_matches(pattern, path))),
            PolicySubject::Edit(patterns) => self
                .edits
                .iter()
                .any(|path| patterns.iter().any(|pattern| path_matches(pattern, path))),
            PolicySubject::Mcp { server, tool } => self
                .mcp
                .iter()
                .any(|(s, t)| wildcard_matches(server, s) && wildcard_matches(tool, t)),
        })
    }
}

/// Find the project a policy was installed into: the nearest directory, at
/// or above one of `starts`, holding `<dir_prefix>/policies/<id>/policy.toml`.
pub fn find_policy_root(starts: &[PathBuf], dir_prefix: &str, policy_id: &str) -> Option<PathBuf> {
    let record = Path::new(dir_prefix)
        .join("policies")
        .join(policy_id)
        .join("policy.toml");
    starts.iter().find_map(|start| {
        start
            .ancestors()
            .find(|dir| dir.join(&record).is_file())
            .map(Path::to_path_buf)
    })
}

/// Read the `[policy]` section of an installed `policy.toml` record.
pub fn load_installed_policy(path: &Path) -> Result<PolicyConfig> {
    #[derive(serde::Deserialize)]
    struct Record {
        policy: PolicyConfig,
    }
    let text = std::fs::read_to_string(path).map_err(|error| {
        TuffError::not_found(format!("cannot read {}: {error}", path.display()))
    })?;
    let record: Record = toml::from_str(&text).map_err(|error| {
        TuffError::corrupt(format!(
            "{} is not a policy record: {error}",
            path.display()
        ))
    })?;
    crate::policy::validate_policy(&record.policy)?;
    Ok(record.policy)
}

// ── commands ───────────────────────────────────────────────────────────

/// Whether a policy command rule, such as `["git", "push", "--force"]`,
/// matches one program invocation.
///
/// The program is compared by name. Options between the program and the
/// rule's first word are skipped, with the value of an option known to
/// take one (`git -C <dir>`), so `git -C . push --force` matches. The
/// rule's first word after the program must be the first argument left;
/// its later words must follow in order, anywhere after it, so
/// `git push origin --force` matches too. A rule word that is a cluster of
/// short options, such as `-rf`, matches when each letter is set by a short
/// option cluster after it (`-r -f`, `-fr`).
pub fn command_matches(rule: &[String], argv: &[String]) -> bool {
    let (Some(program), Some(invoked)) = (rule.first(), argv.first()) else {
        return false;
    };
    if program_name(invoked) != *program {
        return false;
    }
    let rest = &rule[1..];
    let Some(first) = rest.first() else {
        return true;
    };
    let mut position = 1;
    if !first.starts_with('-') {
        while let Some(arg) = argv.get(position) {
            if arg == first || !arg.starts_with('-') {
                break;
            }
            if arg == "--" {
                position += 1;
                break;
            }
            position += if option_takes_value(program, arg) {
                2
            } else {
                1
            };
        }
        if argv.get(position) != Some(first) {
            return false;
        }
        position += 1;
        return words_follow(&rest[1..], &argv[position.min(argv.len())..]);
    }
    words_follow(rest, &argv[position..])
}

fn words_follow(words: &[String], args: &[String]) -> bool {
    let mut remaining = args;
    for word in words {
        if let Some(letters) = short_cluster(word) {
            let set: Vec<char> = remaining
                .iter()
                .filter_map(|arg| short_cluster(arg))
                .flat_map(str::chars)
                .collect();
            if !letters.chars().all(|letter| set.contains(&letter)) {
                return false;
            }
            continue;
        }
        match remaining.iter().position(|arg| arg == word) {
            Some(found) => remaining = &remaining[found + 1..],
            None => return false,
        }
    }
    true
}

/// The letters of a short option cluster such as `-rf`.
fn short_cluster(word: &str) -> Option<&str> {
    let letters = word.strip_prefix('-')?;
    (!letters.is_empty()
        && !letters.starts_with('-')
        && letters.chars().all(|c| c.is_ascii_alphanumeric()))
    .then_some(letters)
}

/// Options that come before a subcommand and take the next argument as
/// their value, for the programs policies most often name.
fn option_takes_value(program: &str, option: &str) -> bool {
    if option.contains('=') {
        return false;
    }
    let options: &[&str] = match program {
        "git" => &[
            "-C",
            "-c",
            "--git-dir",
            "--work-tree",
            "--namespace",
            "--super-prefix",
            "--config-env",
        ],
        "docker" | "podman" => &[
            "-H",
            "--host",
            "-c",
            "--context",
            "--config",
            "-l",
            "--log-level",
        ],
        "kubectl" | "oc" => &[
            "-n",
            "--namespace",
            "--context",
            "--cluster",
            "--kubeconfig",
            "-s",
            "--server",
            "--user",
            "--token",
        ],
        "helm" => &["-n", "--namespace", "--kube-context", "--kubeconfig"],
        "npm" | "pnpm" | "yarn" => &[
            "-C",
            "--prefix",
            "--dir",
            "--cwd",
            "--filter",
            "-w",
            "--workspace",
        ],
        "cargo" => &["-C", "--config", "-Z", "--color"],
        "gh" => &["-R", "--repo"],
        "aws" => &["--profile", "--region", "--output", "--endpoint-url"],
        "gcloud" => &["--project", "--account", "--configuration"],
        _ => &[],
    };
    options.contains(&option)
}

fn program_name(word: &str) -> &str {
    word.rsplit('/').next().unwrap_or(word)
}

/// One program invocation in a shell command line.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct ShellCommand {
    argv: Vec<String>,
    /// Targets of `<` redirections.
    inputs: Vec<String>,
    /// Targets of `>`, `>>`, and similar redirections.
    outputs: Vec<String>,
}

/// Every program invocation a command line runs, as far as the command
/// line itself says: wrappers are unwrapped and substitutions read.
fn shell_commands(script: &str) -> Vec<ShellCommand> {
    let mut commands = Vec::new();
    collect_commands(script, 0, &mut commands);
    commands
}

fn collect_commands(script: &str, depth: usize, out: &mut Vec<ShellCommand>) {
    let (parsed, substitutions) = parse_script(script);
    if depth < MAX_DEPTH {
        for substitution in substitutions {
            collect_commands(&substitution, depth + 1, out);
        }
    }
    for command in parsed {
        unwrap_command(command, depth, out);
    }
}

/// Words that start or shape a compound command rather than name a program.
const RESERVED: &[&str] = &[
    "if", "then", "else", "elif", "fi", "do", "done", "while", "until", "!", "{", "}", "case",
    "esac", "for", "select", "function", "time", "coproc",
];

fn unwrap_command(mut command: ShellCommand, depth: usize, out: &mut Vec<ShellCommand>) {
    loop {
        let skip = command
            .argv
            .iter()
            .take_while(|word| RESERVED.contains(&word.as_str()) || is_assignment(word))
            .count();
        command.argv.drain(..skip);
        let Some(first) = command.argv.first() else {
            out.push(command);
            return;
        };
        let name = program_name(first).to_string();
        let args = &command.argv[1..];
        let inner: Option<Unwrapped> = match name.as_str() {
            "env" => Some(unwrap_env(args)),
            "sudo" => Some(Unwrapped::Argv(skip_options(
                args,
                &["-u", "-g", "-h", "-p", "-C", "-D", "-r", "-t", "-U", "-T"],
            ))),
            "doas" => Some(Unwrapped::Argv(skip_options(args, &["-u", "-C"]))),
            "nice" => Some(Unwrapped::Argv(skip_options(args, &["-n"]))),
            "ionice" => Some(Unwrapped::Argv(skip_options(args, &["-c", "-n", "-p"]))),
            "stdbuf" => Some(Unwrapped::Argv(skip_options(args, &["-i", "-o", "-e"]))),
            "exec" => Some(Unwrapped::Argv(skip_options(args, &["-a"]))),
            "command" | "builtin" | "nohup" | "setsid" | "unbuffer" | "caffeinate" => {
                Some(Unwrapped::Argv(skip_options(args, &[])))
            }
            "timeout" | "gtimeout" => {
                let rest = skip_options(args, &["-s", "--signal", "-k", "--kill-after"]);
                Some(Unwrapped::Argv(rest.into_iter().skip(1).collect()))
            }
            "xargs" => Some(Unwrapped::Argv(skip_options(
                args,
                &[
                    "-I", "-i", "-n", "-P", "-d", "-L", "-l", "-s", "-E", "-e", "-a",
                ],
            ))),
            "watch" => Some(Unwrapped::Script(
                skip_options(args, &["-n", "--interval", "-d"]).join(" "),
            )),
            "busybox" => Some(Unwrapped::Argv(args.to_vec())),
            "eval" => Some(Unwrapped::Script(args.join(" "))),
            "sh" | "bash" | "zsh" | "dash" | "ksh" | "mksh" | "ash" | "fish" => {
                shell_script_argument(args).map(Unwrapped::Script)
            }
            _ => None,
        };
        match inner {
            None => {
                out.push(command);
                return;
            }
            Some(Unwrapped::Argv(argv)) => {
                // Keep the wrapper itself too: a rule may name `sudo`.
                out.push(ShellCommand {
                    argv: command.argv.clone(),
                    ..ShellCommand::default()
                });
                command.argv = argv;
            }
            Some(Unwrapped::Script(script)) => {
                out.push(command);
                if depth < MAX_DEPTH {
                    collect_commands(&script, depth + 1, out);
                }
                return;
            }
        }
    }
}

enum Unwrapped {
    Argv(Vec<String>),
    Script(String),
}

fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty()
            && !name.starts_with(|c: char| c.is_ascii_digit())
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// The arguments after a wrapper's own options, skipping the value of each
/// option in `with_value`.
fn skip_options(args: &[String], with_value: &[&str]) -> Vec<String> {
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if arg == "--" {
            index += 1;
            break;
        }
        if !arg.starts_with('-') || arg == "-" {
            break;
        }
        index += if with_value.contains(&arg.as_str()) {
            2
        } else {
            1
        };
    }
    args.get(index..)
        .map(<[String]>::to_vec)
        .unwrap_or_default()
}

fn unwrap_env(args: &[String]) -> Unwrapped {
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        match arg.as_str() {
            "-S" | "--split-string" => {
                return Unwrapped::Script(args[index + 1..].join(" "));
            }
            "-u" | "--unset" | "-C" | "--chdir" => index += 2,
            "--" => {
                index += 1;
                break;
            }
            _ if arg.starts_with('-') || is_assignment(arg) => index += 1,
            _ => break,
        }
    }
    Unwrapped::Argv(
        args.get(index..)
            .map(<[String]>::to_vec)
            .unwrap_or_default(),
    )
}

/// The script of `sh -c <script>` and its spellings (`bash -lc`, `-e -c`).
fn shell_script_argument(args: &[String]) -> Option<String> {
    let mut reads_script = false;
    for arg in args {
        if reads_script {
            return Some(arg.clone());
        }
        if arg == "-c" || (short_cluster(arg).is_some_and(|letters| letters.contains('c'))) {
            reads_script = true;
        } else if arg == "-o" || arg == "+o" {
            continue;
        } else if !arg.starts_with('-') && !arg.starts_with('+') {
            return None;
        }
    }
    None
}

/// Split a command line into simple commands the way a POSIX shell reads
/// it: quotes and escapes, the operators that separate commands, and
/// redirections. Returns the commands and the text of every `$(...)` and
/// backquote substitution, to be read as command lines of their own.
fn parse_script(script: &str) -> (Vec<ShellCommand>, Vec<String>) {
    let chars: Vec<char> = script.chars().collect();
    let mut commands = Vec::new();
    let mut substitutions = Vec::new();
    let mut current = ShellCommand::default();
    let mut word = String::new();
    let mut in_word = false;
    let mut redirect: Option<bool> = None; // Some(true) = output target next
    let mut heredocs: Vec<(String, bool)> = Vec::new();
    let mut i = 0;

    let finish_word = |word: &mut String,
                       in_word: &mut bool,
                       redirect: &mut Option<bool>,
                       current: &mut ShellCommand,
                       heredocs: &mut Vec<(String, bool)>,
                       heredoc_pending: &mut Option<bool>| {
        if !*in_word {
            return;
        }
        let text = std::mem::take(word);
        *in_word = false;
        if let Some(strip_tabs) = heredoc_pending.take() {
            heredocs.push((text, strip_tabs));
            return;
        }
        match redirect.take() {
            Some(true) => current.outputs.push(text),
            Some(false) => current.inputs.push(text),
            None => current.argv.push(text),
        }
    };
    let mut heredoc_pending: Option<bool> = None;

    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' => {
                finish_word(
                    &mut word,
                    &mut in_word,
                    &mut redirect,
                    &mut current,
                    &mut heredocs,
                    &mut heredoc_pending,
                );
                i += 1;
            }
            '\n' | ';' | '&' | '|' | '(' | ')' => {
                finish_word(
                    &mut word,
                    &mut in_word,
                    &mut redirect,
                    &mut current,
                    &mut heredocs,
                    &mut heredoc_pending,
                );
                if c == '&' && chars.get(i + 1) == Some(&'>') {
                    // `&>` and `&>>`: both streams to a file.
                    i += if chars.get(i + 2) == Some(&'>') { 3 } else { 2 };
                    redirect = Some(true);
                    continue;
                }
                if c == '|' && chars.get(i + 1) == Some(&'&') {
                    i += 1;
                }
                commands.push(std::mem::take(&mut current));
                redirect = None;
                i += 1;
                if c == '\n' {
                    i = skip_heredoc_bodies(&chars, i, &mut heredocs);
                }
            }
            '#' if !in_word => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '<' | '>' => {
                // A word of digits right before is a file descriptor.
                if in_word && word.chars().all(|d| d.is_ascii_digit()) {
                    word.clear();
                    in_word = false;
                } else {
                    finish_word(
                        &mut word,
                        &mut in_word,
                        &mut redirect,
                        &mut current,
                        &mut heredocs,
                        &mut heredoc_pending,
                    );
                }
                let output = c == '>';
                i += 1;
                if !output && chars.get(i) == Some(&'<') {
                    i += 1;
                    if chars.get(i) == Some(&'<') {
                        // `<<<` here-string: its word is data.
                        i += 1;
                        redirect = None;
                        heredoc_pending = None;
                        // Read the word and drop it.
                        let (_, next) = read_word(&chars, i, &mut substitutions);
                        i = next;
                        continue;
                    }
                    let strip_tabs = chars.get(i) == Some(&'-');
                    if strip_tabs {
                        i += 1;
                    }
                    heredoc_pending = Some(strip_tabs);
                    continue;
                }
                if matches!(chars.get(i), Some('>' | '|')) {
                    i += 1;
                }
                if chars.get(i) == Some(&'&') {
                    // `>&2`, `<&0`: duplicating a descriptor, no file.
                    i += 1;
                    while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '-') {
                        i += 1;
                    }
                    continue;
                }
                redirect = Some(output);
            }
            _ => {
                let (text, next) = read_word(&chars, i, &mut substitutions);
                word.push_str(&text);
                in_word = true;
                i = next;
            }
        }
    }
    finish_word(
        &mut word,
        &mut in_word,
        &mut redirect,
        &mut current,
        &mut heredocs,
        &mut heredoc_pending,
    );
    commands.push(current);
    commands.retain(|command| {
        !command.argv.is_empty() || !command.inputs.is_empty() || !command.outputs.is_empty()
    });
    (commands, substitutions)
}

/// After a newline, skip the bodies of pending here-documents.
fn skip_heredoc_bodies(chars: &[char], mut i: usize, heredocs: &mut Vec<(String, bool)>) -> usize {
    for (delimiter, strip_tabs) in heredocs.drain(..) {
        loop {
            if i >= chars.len() {
                return i;
            }
            let end = chars[i..]
                .iter()
                .position(|&c| c == '\n')
                .map_or(chars.len(), |p| i + p);
            let line: String = chars[i..end].iter().collect();
            i = (end + 1).min(chars.len());
            let line = if strip_tabs {
                line.trim_start_matches('\t')
            } else {
                line.as_str()
            };
            if line == delimiter {
                break;
            }
        }
    }
    i
}

/// Read the part of a word that starts at `i` and runs to the next
/// unquoted blank or operator, removing quotes and escapes. Substitutions
/// are recorded and kept in the word as written.
fn read_word(chars: &[char], mut i: usize, substitutions: &mut Vec<String>) -> (String, usize) {
    let mut text = String::new();
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' | '\n' | ';' | '&' | '|' | '(' | ')' | '<' | '>' => break,
            '\\' => {
                if let Some(&next) = chars.get(i + 1)
                    && next != '\n'
                {
                    text.push(next);
                }
                i += 2;
            }
            '\'' => {
                i += 1;
                while i < chars.len() && chars[i] != '\'' {
                    text.push(chars[i]);
                    i += 1;
                }
                i += 1;
            }
            '"' => {
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    match chars[i] {
                        '\\' if matches!(chars.get(i + 1), Some('"' | '\\' | '$' | '`' | '\n')) => {
                            if chars[i + 1] != '\n' {
                                text.push(chars[i + 1]);
                            }
                            i += 2;
                        }
                        '$' if chars.get(i + 1) == Some(&'(') && chars.get(i + 2) != Some(&'(') => {
                            let (inner, next) = balanced(chars, i + 2);
                            text.push_str(&format!("$({inner})"));
                            substitutions.push(inner);
                            i = next;
                        }
                        '`' => {
                            let (inner, next) = backquoted(chars, i + 1);
                            text.push_str(&format!("`{inner}`"));
                            substitutions.push(inner);
                            i = next;
                        }
                        other => {
                            text.push(other);
                            i += 1;
                        }
                    }
                }
                i += 1;
            }
            '$' if chars.get(i + 1) == Some(&'\'') => {
                i += 2;
                while i < chars.len() && chars[i] != '\'' {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        text.push(chars[i + 1]);
                        i += 2;
                    } else {
                        text.push(chars[i]);
                        i += 1;
                    }
                }
                i += 1;
            }
            '$' if chars.get(i + 1) == Some(&'(') => {
                if chars.get(i + 2) == Some(&'(') {
                    // Arithmetic `$((...))`: no command inside to read.
                    let (inner, next) = balanced(chars, i + 2);
                    text.push_str(&format!("$({inner})"));
                    i = next;
                } else {
                    let (inner, next) = balanced(chars, i + 2);
                    text.push_str(&format!("$({inner})"));
                    substitutions.push(inner);
                    i = next;
                }
            }
            '`' => {
                let (inner, next) = backquoted(chars, i + 1);
                text.push_str(&format!("`{inner}`"));
                substitutions.push(inner);
                i = next;
            }
            other => {
                text.push(other);
                i += 1;
            }
        }
    }
    (text, i)
}

/// The text up to the parenthesis that closes one opened just before `i`,
/// and the position after it.
fn balanced(chars: &[char], mut i: usize) -> (String, usize) {
    let start = i;
    let mut depth = 1;
    let mut quote: Option<char> = None;
    while i < chars.len() {
        let c = chars[i];
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some('"'), '\\') => i += 1,
            (Some(_), _) => {}
            (None, '\\') => i += 1,
            (None, '\'' | '"') => quote = Some(c),
            (None, '(') => depth += 1,
            (None, ')') => {
                depth -= 1;
                if depth == 0 {
                    return (chars[start..i].iter().collect(), i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    (
        chars[start..chars.len().min(i)].iter().collect(),
        chars.len(),
    )
}

fn backquoted(chars: &[char], mut i: usize) -> (String, usize) {
    let mut inner = String::new();
    while i < chars.len() && chars[i] != '`' {
        if chars[i] == '\\' && i + 1 < chars.len() {
            inner.push(chars[i + 1]);
            i += 2;
        } else {
            inner.push(chars[i]);
            i += 1;
        }
    }
    (inner, (i + 1).min(chars.len()))
}

// ── files a command names ──────────────────────────────────────────────

/// Programs whose arguments are files they read.
const READERS: &[&str] = &[
    "cat",
    "tac",
    "head",
    "tail",
    "less",
    "more",
    "bat",
    "batcat",
    "nl",
    "od",
    "xxd",
    "hexdump",
    "strings",
    "base64",
    "sed",
    "awk",
    "gawk",
    "grep",
    "egrep",
    "fgrep",
    "rg",
    "ag",
    "cut",
    "sort",
    "uniq",
    "wc",
    "diff",
    "cmp",
    "md5sum",
    "md5",
    "sha1sum",
    "sha256sum",
    "shasum",
    "file",
    "jq",
    "yq",
    "source",
    ".",
    "column",
    "fold",
    "paste",
    "iconv",
    "openssl",
    "gpg",
    "zcat",
    "view",
    "vim",
    "vi",
    "nano",
    "emacs",
    "open",
];

/// Programs whose arguments are files they write, create, or remove.
const WRITERS: &[&str] = &[
    "tee", "touch", "rm", "unlink", "truncate", "shred", "rmdir", "mkdir",
];

/// The files one program invocation reads and writes, as its command line
/// names them.
fn command_files(command: &ShellCommand) -> (Vec<String>, Vec<String>) {
    let mut reads = command.inputs.clone();
    let mut edits: Vec<String> = command
        .outputs
        .iter()
        .filter(|target| !target.starts_with("/dev/"))
        .cloned()
        .collect();
    let Some(first) = command.argv.first() else {
        return (reads, edits);
    };
    let name = program_name(first);
    let operands: Vec<&String> = command.argv[1..]
        .iter()
        .filter(|arg| !arg.starts_with('-'))
        .collect();
    let owned = |items: &[&String]| items.iter().map(|item| (*item).clone()).collect::<Vec<_>>();
    if READERS.contains(&name) {
        reads.extend(owned(&operands));
        let in_place = command.argv[1..].iter().any(|arg| {
            arg == "-i" || (arg.starts_with("-i") && name == "sed") || arg == "--in-place"
        });
        if in_place {
            edits.extend(owned(&operands));
        }
    } else if WRITERS.contains(&name)
        || name == "mv"
        || (name == "perl"
            && command
                .argv
                .iter()
                .any(|arg| arg.starts_with("-i") || arg.starts_with("-pi")))
    {
        edits.extend(owned(&operands));
    } else if matches!(name, "cp" | "rsync" | "scp" | "install" | "ln") {
        if let Some((last, sources)) = operands.split_last() {
            reads.extend(owned(sources));
            edits.push((*last).clone());
        }
    } else if name == "dd" {
        for arg in &command.argv[1..] {
            if let Some(path) = arg.strip_prefix("if=") {
                reads.push(path.to_string());
            } else if let Some(path) = arg.strip_prefix("of=") {
                edits.push(path.to_string());
            }
        }
    }
    (reads, edits)
}

// ── paths ──────────────────────────────────────────────────────────────

/// A path as the project-relative, `/`-separated form policy patterns are
/// written against, or `None` for a path outside the project.
fn project_relative(path: &str, context: EvalContext<'_>) -> Option<String> {
    let path = path.strip_prefix("file://").unwrap_or(path);
    let joined = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        context.cwd.join(path)
    };
    let normalized = lexical_normalize(&joined);
    let mut roots = vec![lexical_normalize(context.root)];
    if let Ok(canonical) = context.root.canonicalize() {
        roots.push(canonical);
    }
    let mut candidates = vec![normalized.clone()];
    if let Some(canonical) = canonicalize_existing_prefix(&normalized) {
        candidates.push(canonical);
    }
    for candidate in &candidates {
        for root in &roots {
            if let Ok(relative) = candidate.strip_prefix(root) {
                let parts: Vec<String> = relative
                    .components()
                    .filter_map(|component| match component {
                        Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
                        _ => None,
                    })
                    .collect();
                if parts.is_empty() {
                    return None;
                }
                return Some(parts.join("/"));
            }
        }
    }
    None
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Resolve symbolic links in the part of `path` that exists, so a link
/// into or out of the project is judged by where it leads.
fn canonicalize_existing_prefix(path: &Path) -> Option<PathBuf> {
    let mut existing = path.to_path_buf();
    let mut rest = Vec::new();
    while !existing.exists() {
        rest.push(existing.file_name()?.to_os_string());
        existing.pop();
    }
    let mut resolved = existing.canonicalize().ok()?;
    for part in rest.into_iter().rev() {
        resolved.push(part);
    }
    Some(resolved)
}

/// Whether a policy path pattern covers a project-relative path, read the
/// way `.gitignore` reads a pattern in a file at the project root: a
/// pattern with a `/` before its end is anchored at the root, one without
/// matches at any depth, a trailing `/` names a directory's contents, and a
/// pattern that matches a directory covers everything in it. `*` and `?`
/// stay within one path segment; `**` spans any number of them.
pub fn path_matches(pattern: &str, path: &str) -> bool {
    let pattern = pattern.trim_start_matches("./");
    let directory_only = pattern.ends_with('/');
    let pattern = pattern.trim_end_matches('/');
    let mut pattern_segments: Vec<&str> = pattern.split('/').collect();
    if !pattern.contains('/') {
        pattern_segments.insert(0, "**");
    }
    let path_segments: Vec<&str> = path.split('/').collect();
    let last = if directory_only {
        path_segments.len().saturating_sub(1)
    } else {
        path_segments.len()
    };
    (1..=last).any(|end| segments_match(&pattern_segments, &path_segments[..end]))
}

fn segments_match(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => (0..=path.len()).any(|skip| segments_match(rest, &path[skip..])),
        Some((first, rest)) => path.split_first().is_some_and(|(segment, path_rest)| {
            wildcard_matches(first, segment) && segments_match(rest, path_rest)
        }),
    }
}

/// `*` for any run of characters and `?` for one, within one segment.
pub fn wildcard_matches(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, t));
            p += 1;
        } else if let Some((star_p, star_t)) = star {
            p = star_p + 1;
            t = star_t + 1;
            star = Some((star_p, star_t + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_string).collect()
    }

    fn argvs(script: &str) -> Vec<Vec<String>> {
        shell_commands(script)
            .into_iter()
            .map(|command| command.argv)
            .filter(|argv| !argv.is_empty())
            .collect()
    }

    fn denies(rule: &str, script: &str) -> bool {
        let rule = words(rule);
        argvs(script)
            .iter()
            .any(|argv| command_matches(&rule, argv))
    }

    #[test]
    fn reworded_commands_from_issue_41_are_matched() {
        let rule = "git push --force";
        assert!(denies(rule, "git push --force"));
        assert!(denies(rule, "/usr/bin/git push --force"));
        assert!(denies(rule, "sh -c \"git push --force\""));
        assert!(denies(rule, "bash -lc 'git push --force origin main'"));
        assert!(denies(rule, "git -C . push --force"));
        assert!(denies(
            rule,
            "git -c core.pager=cat --no-pager push --force"
        ));
        assert!(denies(rule, "git push origin main --force"));
    }

    #[test]
    fn compound_commands_are_split() {
        let rule = "git push --force";
        assert!(denies(rule, "cd repo && git push --force"));
        assert!(denies(rule, "true || git push --force"));
        assert!(denies(rule, "echo hi; git push --force"));
        assert!(denies(rule, "echo hi | git push --force"));
        assert!(denies(rule, "(cd repo; git push --force) &"));
        assert!(denies(rule, "echo $(git push --force)"));
        assert!(denies(rule, "echo \"`git push --force`\""));
        assert!(denies(rule, "if true; then git push --force; fi"));
    }

    #[test]
    fn wrappers_are_unwrapped() {
        let rule = "git push --force";
        assert!(denies(rule, "env GIT_TRACE=1 git push --force"));
        assert!(denies(rule, "GIT_TRACE=1 git push --force"));
        assert!(denies(rule, "sudo -u deploy git push --force"));
        assert!(denies(rule, "nohup timeout 30 git push --force"));
        assert!(denies(rule, "command git push --force"));
        assert!(denies(rule, "eval git push --force"));
        assert!(denies(rule, "env -S 'git push --force'"));
        assert!(denies(rule, "xargs -n 1 git push --force"));
        assert!(denies("sudo", "sudo -u deploy ls"));
    }

    #[test]
    fn other_commands_are_left_alone() {
        let rule = "git push --force";
        assert!(!denies(rule, "git push"));
        assert!(!denies(rule, "git status --force"));
        assert!(!denies(rule, "echo 'git push --force'"));
        assert!(!denies(rule, "git push --force-with-lease"));
        assert!(!denies(rule, "legit push --force"));
        assert!(!denies(rule, "cat <<EOF\ngit push --force\nEOF"));
        assert!(!denies(rule, "# git push --force"));
    }

    #[test]
    fn short_option_clusters_match_in_any_spelling() {
        assert!(denies("rm -rf", "rm -rf build"));
        assert!(denies("rm -rf", "rm -fr build"));
        assert!(denies("rm -rf", "rm -r -f build"));
        assert!(!denies("rm -rf", "rm -r build"));
        assert!(denies(
            "terraform apply",
            "terraform -chdir=infra apply -auto-approve"
        ));
        assert!(denies(
            "kubectl delete namespace",
            "kubectl -n prod delete namespace prod"
        ));
    }

    #[test]
    fn files_named_on_the_command_line_are_found() {
        let files = |script: &str| {
            let mut reads = Vec::new();
            let mut edits = Vec::new();
            for command in shell_commands(script) {
                let (r, e) = command_files(&command);
                reads.extend(r);
                edits.extend(e);
            }
            (reads, edits)
        };
        assert_eq!(files("cat .env").0, vec![".env"]);
        assert_eq!(files("head -n 5 config/.env").0, vec!["5", "config/.env"]);
        assert_eq!(files("wc -l < secrets/key").0, vec!["secrets/key"]);
        assert_eq!(files("echo x > .env 2>/dev/null").1, vec![".env"]);
        assert_eq!(files("echo x >> out.txt").1, vec!["out.txt"]);
        assert_eq!(files("echo x 2>&1").1, Vec::<String>::new());
        assert_eq!(files("sed -i 's/a/b/' .env").1, vec!["s/a/b/", ".env"]);
        assert_eq!(
            files("cp .env /tmp/x"),
            (vec![".env".to_string()], vec!["/tmp/x".to_string()])
        );
        assert_eq!(files("sh -c 'cat .env'").0, vec![".env"]);
    }

    #[test]
    fn path_patterns_follow_gitignore_rules() {
        assert!(path_matches(".env", ".env"));
        assert!(path_matches(".env", "app/.env"));
        assert!(!path_matches(".env", ".env.example"));
        assert!(path_matches(".env*", ".env.local"));
        assert!(path_matches("secrets/**", "secrets/prod/key"));
        assert!(path_matches("secrets/", "secrets/key"));
        assert!(!path_matches("secrets/", "secrets"));
        assert!(path_matches("secrets", "secrets/key"));
        assert!(!path_matches("secrets/**", "app/secrets/key"));
        assert!(path_matches("config/*.pem", "config/tls.pem"));
        assert!(!path_matches("config/*.pem", "config/sub/tls.pem"));
        assert!(path_matches("**/*.pem", "a/b/c.pem"));
        assert!(path_matches("infra/", "infra/main.tf"));
    }

    #[test]
    fn paths_are_judged_relative_to_the_project() {
        let root = Path::new("/work/project");
        let context = EvalContext {
            root,
            cwd: Path::new("/work/project/app"),
        };
        assert_eq!(
            project_relative(".env", context).as_deref(),
            Some("app/.env")
        );
        assert_eq!(
            project_relative("../.env", context).as_deref(),
            Some(".env")
        );
        assert_eq!(
            project_relative("/work/project/secrets/key", context).as_deref(),
            Some("secrets/key")
        );
        assert_eq!(project_relative("/etc/passwd", context), None);
        assert_eq!(project_relative("../../other/.env", context), None);
    }

    fn policy(toml_rules: &str) -> PolicyConfig {
        toml::from_str(toml_rules).unwrap()
    }

    #[test]
    fn deny_wins_over_ask_and_the_first_rule_is_named() {
        let policy = policy(
            r#"
            [[rules]]
            effect = "ask"
            command = ["git", "push"]

            [[rules]]
            effect = "deny"
            command = ["git", "push", "--force"]
            reason = "rewrites shared history"

            [[rules]]
            effect = "deny"
            read = [".env"]

            [[rules]]
            effect = "deny"
            mcp = "github:delete_*"
            "#,
        );
        let root = Path::new("/work/project");
        let context = EvalContext { root, cwd: root };
        let decide =
            |actions: &[PolicyAction]| evaluate("infra", &policy, actions, context).unwrap();

        let decision = decide(&[PolicyAction::Shell("git push --force".into())]).unwrap();
        assert_eq!((decision.effect, decision.rule), (PolicyEffect::Deny, 2));
        assert_eq!(
            decision.message(),
            "Tuff policy 'infra' denies this call: rule 2 (deny command \"git push --force\"): rewrites shared history"
        );
        let decision = decide(&[PolicyAction::Shell("git push".into())]).unwrap();
        assert_eq!((decision.effect, decision.rule), (PolicyEffect::Ask, 1));
        assert_eq!(decide(&[PolicyAction::Shell("git status".into())]), None);
        assert_eq!(
            decide(&[PolicyAction::Read("/work/project/app/.env".into())])
                .unwrap()
                .rule,
            3
        );
        assert_eq!(
            decide(&[PolicyAction::Shell("cat app/.env".into())])
                .unwrap()
                .rule,
            3
        );
        assert_eq!(
            decide(&[PolicyAction::Read("/elsewhere/.env".into())]),
            None
        );
        let mcp = |server: &str, tool: &str| PolicyAction::Mcp {
            server: server.into(),
            tool: tool.into(),
        };
        assert_eq!(decide(&[mcp("github", "delete_repo")]).unwrap().rule, 4);
        assert_eq!(decide(&[mcp("github", "create_issue")]), None);
    }
}
