# Tuff Hooks Specification

Version 0.2.0. This document describes the hook vocabulary Tuff implements, how a hook written against it is rendered into each supported harness, and what a second implementation has to do to be compatible. It is generated in part: the tables below come from the same constants Tuff runs on, produced by `tuff hooks spec --json`, and the [machine-readable document](https://github.com/kannandreams/tuff/blob/main/spec/hooks/hooks-spec.json) and its [JSON Schema](https://github.com/kannandreams/tuff/blob/main/spec/hooks/hooks-spec.schema.json) sit beside this file. On tuffcli.dev the same two files are served at `/spec/hooks/hooks-spec.json` and `/spec/hooks/hooks-spec.schema.json`.

This version was tested by building a second implementation from the specification alone, without access to Tuff's source, and comparing what it writes against Tuff for every harness and event name. Every place that implementation had to guess is now stated below. The [conformance kit](https://github.com/kannandreams/tuff/tree/main/spec/hooks/conformance) holds that implementation and the comparison, and section 8 describes how to run it against your own.

The words MUST, SHOULD, and MAY are used as in RFC 2119.

## 1. Scope

A hook is automation a coding harness runs at a defined moment in a session: before a tool call, after a file is saved, when the agent is about to stop. Every harness invented its own event names, its own settings file, and its own idea of what a hook may block. This specification defines one vocabulary above them, a way to declare honestly how well each harness can honour it, and the files an implementation writes so that a hook authored once installs anywhere.

It covers Tuff-standard hooks: a hook declared with a canonical event name. It does not cover native hook fragments, which a harness's own format describes; an implementation passes those through unchanged, and section 6 says only how they are validated and merged.

## 2. Terms

- **Canonical event.** One of the seven event names in section 3. What a hook author writes.
- **Native event.** The name a harness uses for the same moment, such as `PreToolUse` in Claude Code or `preToolUse` in Cursor.
- **Harness.** The coding-agent environment a hook runs in. **Adapter.** The part of an implementation that renders for one harness.
- **Coverage.** How faithfully a harness honours a canonical event: `full`, `partial`, or `unsupported`.
- **Compatibility matrix.** An adapter's declaration of coverage for every canonical event.
- **Hook directory.** `<dir_prefix>/hooks/<id>/`, relative to the project root, where one hook's files are installed for one harness.
- **Wrapper.** The script an implementation generates in the hook directory, which the harness runs.
- **Registration.** The entry in a harness's settings file that makes it run the wrapper on a native event.
- **Hook fragment.** A hooks-only JSON document in a harness's settings shape, used to register or adopt hooks.

## 3. Canonical events

<!-- generated:events -->
| Event | Blocking | Since | Payload fields |
|---|---|---|---|
| `session_start` | not blocking | 0.1.0 | `session_id` (optional), `cwd` (optional) |
| `session_end` | not blocking | 0.1.0 | `session_id` (optional), `cwd` (optional) |
| `pre_tool_use` | blocks action | 0.1.0 | `session_id` (optional), `cwd` (optional) |
| `post_tool_use` | blocks continuation | 0.1.0 | `session_id` (optional), `cwd` (optional) |
| `before_finish` | blocks continuation | 0.1.0 | `session_id` (optional), `cwd` (optional) |
| `after_save` | not blocking | 0.1.0 | `session_id` (optional), `cwd` (optional) |
| `stop` | blocks continuation | 0.1.0 | `session_id` (optional), `cwd` (optional) |
<!-- /generated:events -->

**Blocking.** Each event states what a handler that fails can block. `blocks action` means the harness has not yet performed the thing the event announces, and a failing handler prevents it; `blocks continuation` means the thing has happened and a failing handler stops the harness from going on; `not blocking` means the handler's result is informational. These are the semantics of the canonical event. A harness may offer less: that is what `partial` coverage and its caveat record, and an implementation MUST NOT claim a blocking effect the native event does not have.

**Payload.** The payload descriptor lists fields a harness MAY pass to a handler. In this version every event shares the same two optional fields and none has a required field. An implementation MUST NOT require any payload field, and a handler SHOULD treat every field as absent until it checks.

**Aliases.** A matrix MAY accept other spellings of a canonical event, such as a harness's own native name, so that a hook written against one harness ports to another. Aliases are per harness: a native name works in a manifest only for a harness whose matrix lists it as an alias. Resolution is defined in section 5.

## 4. A Tuff-standard hook

### 4.1 The manifest

A Tuff-standard hook is a directory holding a `tuff.toml` manifest and, optionally, the runtime files it lists:

```toml
id = "format-check"
type = "hook"
version = "1.0.0"
description = "Refuse to finish while the code is not formatted."
files = ["scripts/check-format.sh"]

[hook]
event = "before_finish"
command = "cargo fmt --check"
working_directory = "."
```

An implementation MUST refuse, before writing anything, a manifest that breaks any of these rules:

- `id`, `version`, and `description` are non-empty strings, and `type` is `"hook"`.
- `id` is a relative path of plain names: one or more segments separated by `/`, where no segment is empty, `.`, or `..`, and the whole contains no `\`, no NUL, and no leading or trailing whitespace, meaning any character with the Unicode `White_Space` property. These are the only restrictions; any other character is allowed. `format-check` and `security/format-check` are valid; `../x`, `/x`, `a//b`, and `a/` are not. The id names the hook directory, and it names the directory an implementation removes when the hook is uninstalled, so an id that escapes would aim both outside where they belong.
- `[hook].event` and `[hook].command` are strings that are not empty or only whitespace.
- `[hook].working_directory`, when present, is a relative path with no `..` segment. It defaults to `.`.
- Every entry in `files` is a path relative to the manifest's directory, optionally starting with `./`, made of plain names in the same sense as `id`, with no symbolic link at any point along it, naming a regular file. A capability can come from anyone's repository, and an entry that escapes the directory would read a file from outside it and, because the same relative path is reused for the destination, write it outside the hook directory.

### 4.2 Runtime files

Only the files listed in `files` are installed. Other files in the manifest's directory, including `tuff.toml` itself unless it is listed, are not. An entry listed more than once, including once with a leading `./` and once without, is installed once.

Each listed file is copied to the hook directory under its listed path, with one leading `src/` removed if present: `scripts/check-format.sh` installs as `<dir_prefix>/hooks/<id>/scripts/check-format.sh`, and `src/check.sh` as `<dir_prefix>/hooks/<id>/check.sh`. An implementation MUST refuse a manifest in which two different listed files would be installed under the same path, such as `check.sh` and `src/check.sh`, rather than keep one of them. It MUST also refuse a manifest in which a listed file would be installed as the wrapper's path, `run.sh` at the top of the hook directory, since it would replace the script the registration runs.

**Known limitation.** The command does not run from the hook directory, and the hook directory differs per harness, so a command cannot portably refer to its own runtime files by a relative path. In the example above, `scripts/check-format.sh` is installed but `cargo fmt --check` does not use it. A command that needs a runtime file has to name its installed path for one harness. This undercuts writing a hook once for every harness exactly when a hook ships more than a one-line command, and the second implementation of this specification singled it out for that reason. Closing it needs a way for the command to learn its hook directory, such as a variable the wrapper sets, and is left to a future minor version because it changes the wrapper every existing install carries.

### 4.3 The wrapper

For each harness, an implementation writes the wrapper at `<dir_prefix>/hooks/<id>/run.sh`, with exactly this content, ending in a newline:

```sh
#!/usr/bin/env bash
set -euo pipefail
cd -- '<working_directory>'
exec bash -euo pipefail -c '<command>'
```

Each value is placed between single quotes, with every single quote inside the value written as the five characters `'"'"'`, which closes the quoted string, emits a double-quoted single quote, and reopens it. The value `echo 'hi'` becomes `'echo '"'"'hi'"'"''`. An implementation MUST refuse a `working_directory` or `command` containing NUL, which no shell string can carry. The file's mode is not significant, because the registration runs it through `sh`; Tuff writes it with the default permissions of the process.

`working_directory` is resolved by the shell relative to the directory the harness runs the hook from. An implementation does not control that directory, and the registration itself, `sh <dir_prefix>/hooks/<id>/run.sh`, is a path relative to the project root, so it already relies on the harness running hooks from there.

The wrapper is the contract between the author's command and the harness: the harness runs a fixed, known path, and the author's command and working directory never reach the settings file, where they would otherwise be interpolated into another program's configuration.

### 4.4 Installing

To install a hook for a harness, an implementation first does every check that can refuse, and only then writes anything:

1. Validate the manifest as sections 4.1 and 4.2 say.
2. Resolve the event against the harness's matrix as section 5 says, refusing or warning as it directs.
3. Read the harness's settings file and compute the merged result as section 6.3 says, which refuses a corrupt file.
4. Write the wrapper, the runtime files, and the merged settings file, in any order, and record the registration.

A refusal in steps 1 to 3 MUST leave the project exactly as it was: no file written, no settings file changed, nothing recorded. A failure while writing in step 4, such as a full disk or a name the filesystem cannot represent, is an error rather than a refusal; an implementation SHOULD report which files it had already written. An implementation MUST NOT execute the hook's command, or any runtime file, while installing, updating, or removing a hook. Reinstalling the same hook replaces its files and leaves the settings file unchanged.

## 5. The compatibility model

Every adapter publishes a compatibility matrix: one row per canonical event, in the format of section 8. A row carries the native event name when there is one, the coverage level, the aliases the row answers to, a scope, a caveat, a source, and optional harness version bounds.

**Coverage levels** and what an implementation MUST do at install time:

| Coverage | Meaning | Install behaviour |
|---|---|---|
| `full` | The native event fires at the same moment with at least the canonical blocking effect, for the declared scope. | Install silently. |
| `partial` | The native event is the closest match but differs in scope or blocking, as the row's `scope` and `caveat` say. | Install, and tell the user, quoting the scope and caveat. |
| `unsupported` | The harness has no event for this moment. | Refuse, with the row's caveat when there is one. |

An `unsupported` row MUST NOT name a native event. A `full` or `partial` row MUST.

**Resolution.** Given an event name from a manifest, an implementation MUST first look for the row whose canonical event is that name, and only if none matches look for a row listing it as an alias. This order matters: a harness may use one native name, such as `stop`, for a moment that two canonical events map onto, and the canonical row must win over the alias so that `stop` written in a manifest records `stop` and not `before_finish`. A name that matches an alias on an `unsupported` row resolves to that row and is refused. A name that matches no row at all MUST be refused in the same way as an `unsupported` row. Either refusal SHOULD list the canonical events the harness supports, with the aliases each accepts, so the author can see what to write instead.

**Scope, caveat, source.** `scope` lists the situations the coverage applies to, in the harness's own terms. `caveat` is prose for the user. `source` is where the row's claim can be checked, usually the harness's documentation. A `partial` row SHOULD carry a caveat; an `unsupported` row SHOULD say why.

**Harness version bounds.** `since_harness_version` and `until_harness_version` record the harness releases a row is known to hold for. They are optional and MUST be omitted rather than guessed. No published row uses them yet.

## 6. Per-harness rendering

<!-- generated:matrices -->
### Open Agents (`open-agents`)

Hooks install under `.agents/hooks/<id>/`, registered in `.agents/hook.json` (grouped shape) as `sh .agents/hooks/<id>/run.sh`.

| Canonical | Native | Coverage | Aliases | Scope | Caveat |
|---|---|---|---|---|---|
| `before_finish` | `before_finish` | full |  |  |  |
| `after_save` | `after_save` | full |  |  |  |
| `pre_tool_use` | `pre_tool_execution` | partial | `pre_tool_execution` | local function tools, Bash, Edit, Write, MCP | No harness documents a hook mechanism for the shared .agents layout; the registration is kept for tools that adopt it. |
| `post_tool_use` | `post_tool_execution` | partial | `post_tool_execution` | local function tools, Bash, Edit, Write, MCP | No harness documents a hook mechanism for the shared .agents layout; the registration is kept for tools that adopt it. |
| `session_start` | none | unsupported |  |  | Open Agents hook.json does not currently define a session-start event. |
| `session_end` | none | unsupported |  |  | Open Agents hook.json does not currently define a session-end event. |
| `stop` | none | unsupported |  |  | Open Agents hook.json does not currently define a stop event. |

### Claude (`claude`)

Hooks install under `.claude/hooks/<id>/`, registered in `.claude/settings.json` (grouped shape) as `sh .claude/hooks/<id>/run.sh`.

| Canonical | Native | Coverage | Aliases | Scope | Caveat |
|---|---|---|---|---|---|
| `session_start` | `SessionStart` | full | `SessionStart` | startup, resume, clear, compact, fork | [source](https://code.claude.com/docs/en/hooks) |
| `session_end` | `SessionEnd` | full | `SessionEnd` | session lifecycle | [source](https://code.claude.com/docs/en/hooks) |
| `pre_tool_use` | `PreToolUse` | full | `PreToolUse` | tool calls | [source](https://code.claude.com/docs/en/hooks) |
| `post_tool_use` | `PostToolUse` | full | `PostToolUse` | successful tool calls | [source](https://code.claude.com/docs/en/hooks) |
| `before_finish` | `Stop` | partial |  | main-agent completion | Claude Stop runs after the main agent finishes responding and can request continuation; it does not represent every possible pre-finish boundary. ([source](https://code.claude.com/docs/en/hooks)) |
| `after_save` | none | unsupported | `FileChanged` |  | Claude FileChanged requires watched filenames or paths that Tuff's standard after_save hook cannot currently express. ([source](https://code.claude.com/docs/en/hooks)) |
| `stop` | `Stop` | full | `Stop` | main-agent completion | [source](https://code.claude.com/docs/en/hooks) |

### Codex (`codex`)

Hooks install under `.agents/hooks/<id>/`, registered in `.codex/hooks.json` (grouped shape) as `sh .agents/hooks/<id>/run.sh`.

| Canonical | Native | Coverage | Aliases | Scope | Caveat |
|---|---|---|---|---|---|
| `session_start` | `SessionStart` | full | `SessionStart` | session lifecycle | [source](https://learn.chatgpt.com/docs/hooks) |
| `session_end` | `SessionEnd` | full | `SessionEnd` | session lifecycle | [source](https://learn.chatgpt.com/docs/hooks) |
| `pre_tool_use` | `PreToolUse` | full | `PreToolUse`, `pre_tool_execution` | tool calls | [source](https://learn.chatgpt.com/docs/hooks) |
| `post_tool_use` | `PostToolUse` | full | `PostToolUse`, `post_tool_execution` | tool calls | [source](https://learn.chatgpt.com/docs/hooks) |
| `before_finish` | `Stop` | partial | `before_finish` | main-agent completion | Codex Stop runs after the agent finishes responding and can request continuation; it does not represent every possible pre-finish boundary. ([source](https://learn.chatgpt.com/docs/hooks)) |
| `after_save` | none | unsupported | `after_save` |  | Codex documents no after-save event; PostToolUse on its edit tools is the closest moment. ([source](https://learn.chatgpt.com/docs/hooks)) |
| `stop` | `Stop` | full | `Stop` | main-agent completion | [source](https://learn.chatgpt.com/docs/hooks) |

### Cursor (`cursor`)

Hooks install under `.cursor/hooks/<id>/`, registered in `.cursor/hooks.json` (flat shape) as `sh .cursor/hooks/<id>/run.sh`.

| Canonical | Native | Coverage | Aliases | Scope | Caveat |
|---|---|---|---|---|---|
| `session_start` | `sessionStart` | full |  |  | [source](https://cursor.com/blog/agent-best-practices) |
| `session_end` | `sessionEnd` | full |  | session lifecycle | [source](https://cursor.com/blog/agent-best-practices) |
| `pre_tool_use` | `preToolUse` | full |  | agent tool calls | A native matcher can narrow the tools that receive the hook. ([source](https://cursor.com/blog/agent-best-practices)) |
| `post_tool_use` | `postToolUse` | full |  | agent tool calls | A native matcher can narrow the tools that receive the hook. ([source](https://cursor.com/blog/agent-best-practices)) |
| `after_save` | none | unsupported |  |  | Cursor does not expose a direct after-save hook in this adapter. |
| `before_finish` | `stop` | partial |  | agent completion | Cursor stop can request continuation rather than block a prior action. ([source](https://cursor.com/blog/agent-best-practices)) |
| `stop` | `stop` | full |  | agent completion | [source](https://cursor.com/blog/agent-best-practices) |
<!-- /generated:matrices -->

### 6.1 Settings shapes

Two shapes of settings file exist among the supported harnesses. In the `grouped` shape, used by Claude Code, Open Agents, and Codex, an event holds groups and a group holds typed entries. A Tuff-standard registration adds one group holding one entry:

```json
{"hooks": {"PreToolUse": [{"hooks": [{"type": "command", "command": "sh .claude/hooks/<id>/run.sh"}]}]}}
```

In the `flat` shape, used by Cursor, an event holds entries directly and the file declares a `version`, currently `1`. A Tuff-standard registration adds one entry:

```json
{"version": 1, "hooks": {"preToolUse": [{"command": "sh .cursor/hooks/<id>/run.sh"}]}}
```

In both, the command is exactly `sh <dir_prefix>/hooks/<id>/run.sh`.

### 6.2 Hook fragments

A hook fragment is a JSON object in one of these shapes whose only top-level key is `hooks`, an object, plus an optional `version` in the flat shape. An implementation MUST refuse a fragment carrying any other top-level key, because a whole settings file offered as a fragment would otherwise be merged key by key into the user's real one. A flat fragment MAY omit `version`. Each event in `hooks` MUST be an array.

### 6.3 Merging

The settings file belongs to the user. An implementation MUST merge a registration into it rather than replace it:

- A missing or empty file is treated as `{}` in the grouped shape and `{"version": 1}` in the flat shape.
- A file that is not a JSON object, or whose `hooks` is present and not an object, MUST be refused as corrupt and left unchanged. So MUST a file in which an event the fragment adds to is present and not an array. Events the fragment does not add to are not inspected, and are kept exactly as they are, whatever they hold.
- Every key and every registration already present MUST be kept.
- For each event in the fragment, each group is appended to the event's array unless the array already holds a group equal to it as a JSON value, in which object key order does not matter. So installing the same hook twice leaves the file's content unchanged.
- In the flat shape, `version` MUST be present afterwards; an existing value is kept.

**Formatting** is not significant: two settings files conform to each other when they are equal as JSON. Tuff writes JSON indented by two spaces with object keys in sorted order, so the first write reorders the keys of a file a user wrote by hand, and it writes no trailing newline after a merge and one after a removal. An implementation MAY preserve key order and MAY choose its own formatting, but it MUST write a file that is byte for byte identical when a merge changes nothing.

### 6.4 Recording

For every registered entry it writes, an implementation MUST record:

- the settings file path, relative to the project root;
- the native event;
- the canonical event;
- the command;
- the entry hash: the lowercase hexadecimal SHA-256 of the entry serialised as compact JSON, with object keys sorted, no whitespace, and non-ASCII characters written as UTF-8 rather than escaped.

The entry is the object carrying the `command`: in the grouped shape the typed entry inside the group, such as `{"command":"sh .claude/hooks/<id>/run.sh","type":"command"}`, not the group around it; in the flat shape the entry itself.

The records are what let the implementation later say whether a registration is still present and unchanged. A recorded entry is present when an entry with the same command is found under the recorded event in the recorded settings file; it is unchanged when that entry's hash equals the recorded one. An implementation that does not share Tuff's lockfile MAY store the records however it likes, but it MUST hash as above if it is to read or write Tuff's records.

### 6.5 Removing

To uninstall a hook for a harness, an implementation MUST remove only the registrations it recorded for that harness's settings file, and MUST leave the user's other registrations and other keys alone. It MUST update the settings file before deleting any file, so that if the settings file exists but is not valid JSON, removal stops with the hook's files still in place and the hook still recorded. A settings file that does not exist, or that has no `hooks` object, holds nothing to remove.

1. For each recorded registration, look in the recorded settings file under the recorded native event.
2. In the grouped shape, remove from every group's `hooks` array each entry whose `command` equals the recorded command, then remove every group whose `hooks` array is now empty.
3. In the flat shape, remove each entry whose `command` equals the recorded command.
4. Remove any event left with an empty array.
5. Delete the hook directory, `<dir_prefix>/hooks/<id>/`, with everything in it.

A group that held the recorded entry and another entry the user added keeps the user's entry. An implementation SHOULD also remove each directory left empty between the hook directory and `<dir_prefix>/hooks/`, that one included, which for a nested id such as `security/format-check` includes `security/`.

## 7. Conformance checklist

An implementation conforms to this version of the specification when all of the following hold. Beside each item is the test in Tuff that pins it, so the claim can be checked against running code.

1. **Vocabulary.** It recognises the seven canonical events of section 3 by their canonical names. *(`tuff-hooks-spec`: `hook_event_display_uses_canonical_name`)*
2. **Resolution order.** It resolves a canonical name before any alias, and refuses a name that matches no row. *(`canonical_name_takes_precedence_over_an_earlier_alias`, `aliases_remain_available_when_no_canonical_name_matches`; the conformance kit's `no_such_event` cases)*
3. **Matrix completeness.** Every matrix it publishes lists every canonical event exactly once, names a native event on every supported row and on no unsupported row, and states the spec version it targets. *(`tuff-cli`: `every_adapter_matrix_covers_every_canonical_event_exactly_once`)*
4. **Honest coverage.** It refuses an `unsupported` event, installs a `partial` event with the caveat shown, and installs a `full` event silently. *(`add_hook_renders_canonical_event_to_native_event`, `add_hook_existing_event_rejected_by_adapter`)*
5. **Manifest validation.** It refuses the manifests section 4.1 describes, including an escaping or malformed `id`. *(`tuff-core`: `a_capability_id_is_a_relative_path_of_plain_names`, `a_manifest_with_an_escaping_id_is_refused_at_load`; `tuff-cli`: `a_capability_id_cannot_aim_install_or_delete_outside_the_project`)*
6. **Contained runtime files.** It installs only listed files, installs a repeated entry once, and refuses an entry that escapes the manifest's directory or passes through a symbolic link, two files that would install under the same path, and a listed file that would replace the wrapper. *(`source_files_refuse_a_path_that_climbs_out_of_the_capability`, `source_files_refuse_a_symbolic_link_anywhere_along_the_path`, `a_manifest_file_path_cannot_read_or_write_outside_the_capability`, `a_symbolic_link_in_a_capability_source_is_refused_not_followed`, `listed_files_that_install_to_the_same_path_are_refused_but_a_repeat_is_not`, `two_listed_files_cannot_install_to_the_same_path`, `duplicate_files_fixture_installs_one_emitted_file`, `a_listed_file_cannot_replace_the_hook_wrapper`)*
7. **Refusal changes nothing.** A refused install writes no file, changes no settings file, and records nothing; a removal stopped by a corrupt settings file deletes nothing. *(the tests above, which assert the project and lockfile are unchanged; `deleting_a_hook_with_a_corrupt_settings_file_changes_nothing`)*
8. **No execution.** It never runs the hook's command or a runtime file while installing, updating, or removing it. *(the wrapper is written, never invoked; `hook_script_preserves_shell_sensitive_values` runs it deliberately, in a test)*
9. **Wrapper.** It writes the wrapper of section 4.3 byte for byte, with the quoting it describes, and refuses NUL. *(`hook_script_preserves_shell_sensitive_values`, `hook_script_rejects_nul_bytes`; the conformance kit compares the bytes)*
10. **Fragment validation.** It refuses a fragment with keys beyond `hooks`, and `version` in the flat shape. *(`tuff-core`: `a_whole_settings_file_is_refused_as_a_fragment`)*
11. **Idempotent merge.** Merging the same registration twice leaves the settings file byte for byte unchanged, and keeps everything the user had, in both shapes. *(`merging_the_same_fragment_twice_does_not_duplicate_the_hook`, `merging_keeps_what_the_user_already_had`)*
12. **Records.** It records the five fields of section 6.4 per registered entry, with the hash computed as described. *(`add_hook_renders_canonical_event_to_native_event` checks the record; the conformance kit compares the hashes)*
13. **Surgical removal.** It removes only its own registrations, keeps a user's entry that shared a group, and prunes what it emptied. *(`removal_takes_out_only_tuff_registrations_in_either_shape`)*
14. **Published matrix.** Its matrices are available as a document valid against the schema of section 8. *(`scripts/check-spec.sh` validates `hooks-spec.json` on every check)*
15. **Agreement with Tuff.** The conformance kit reports no disagreement between it and Tuff for every harness and event name in the matrices. *(`scripts/check-spec.sh` runs the kit on every check)*

## 8. Publishing a matrix, and testing an implementation

The machine-readable form of this specification is a JSON document with three members: `spec_version`, `events`, and `adapters`. Tuff prints its own with `tuff hooks spec --json`; the copy beside this file is regenerated from that command and checked on every change. The JSON Schema beside it describes the document, and an implementation publishing its own matrices SHOULD publish them in the same form so that they can be compared and checked with the same tools.

The `events` member is the table of section 3, one object per canonical event with its blocking scope, the spec version that introduced it, and its payload descriptor. The `adapters` member holds one object per harness: its id and display name, the directory prefix it installs under, the settings file path and shape, the command it registers, the spec version its matrix targets, and the matrix rows.

**The conformance kit** in `spec/hooks/conformance/` tests an implementation against Tuff itself. Its `compare.py` installs the same hook, with runtime files, an unlisted file, a quote in the command, and a space in the working directory, into two fresh projects for every harness and every canonical event, alias, and native name in the matrices, plus one name no row knows: once with `tuff` and once with the implementation under test. It then compares the outcome, the hook directory byte for byte, the settings file as JSON, the records including their hashes, the partial-coverage warning, a second install, and removal. The implementation is driven through a small command-line contract described in the kit's README. The kit also holds the clean-room implementation this version was tested with, which passes it.

## 9. Versioning

The specification carries a semantic version, currently 0.2.0, and it moves independently of Tuff's release version:

- A **patch** change rewords or clarifies without changing what a conforming implementation does.
- A **minor** change adds a canonical event, a payload field, or an optional row attribute, or requires an implementation to refuse input that could only cause harm. A hook that installed under the previous minor, and was not hostile, still installs and renders the same way.
- A **major** change removes or renames an event, changes an event's blocking scope, or changes the resolution order or the rendering of section 4.

Compatibility matrices are data about harnesses, not part of the vocabulary. A row may change when a harness changes, without moving the spec version; such changes ship with the implementing release and appear in its changelog. Every published matrix states the spec version it targets.

**Compatibility statement.** Each Tuff release implements exactly one version of this specification:

| Tuff releases | Specification |
|---|---|
| 0.1.2 through 0.9.0 | 0.1.0 |
| 0.10.0 through 0.12.0 | 0.2.0 |

## 10. Changes to this specification

- **Matrix data, 2026-09-16 (Tuff 0.12.0).** The Codex matrix now names the events Codex reads from `.codex/hooks.json` (`SessionStart`, `SessionEnd`, `PreToolUse`, `PostToolUse`, `Stop`; `before_finish` partial through `Stop`; `after_save` unsupported), with the earlier snake_case names kept as aliases; the earlier rows described a file Codex never read. The Open Agents matrix no longer cites Codex's documentation for events no harness documents. Matrices are data about harnesses, so the specification version is unchanged.
- **0.2.0.** Tested by a clean-room implementation, and every gap it found is closed. New requirements: an implementation MUST refuse an `id` that is not a relative path of plain names, a `files` entry that escapes the manifest's directory or passes through a symbolic link, and a listed file that would replace the wrapper; each closes a way for a hostile capability to write, delete, or run something the user was not shown, and Tuff 0.9.0 and earlier do not meet them. Clarified from Tuff's behaviour: only listed files are installed, and where (4.2); the exact quoting in the wrapper and that its mode is not significant (4.3); the order of installation and that a refusal changes nothing (4.4); that a name matching no row is refused and what the message lists (5); that a flat fragment's `version` is optional (6.2); what counts as an equal group, how corrupt files are treated, and that formatting is not significant (6.3); exactly which object the entry hash covers and how it is computed (6.4); and the removal algorithm step by step (6.5). Section 8 describes the conformance kit. A third pass against the final text settled all but one wording point, which section 5 now states: both kinds of refusal list the usable event names. A second pass of the same clean-room implementation against a draft of 0.2.0 closed its original gaps and found five more, all settled here: that every refusal comes before any write, including a corrupt settings file (4.4); that a repeated `files` entry installs once and two entries colliding on one installed path are refused (4.1, 4.2); which parts of a settings file are inspected (6.3); that removal updates the settings file before deleting files and stops on a corrupt one (6.5); and which empty directories removal prunes, and what counts as whitespace in an id. Three of those were also bugs in Tuff, fixed alongside: colliding files silently kept the last one, a hook could list its own `run.sh` and replace the wrapper, and a delete with a corrupt settings file removed the files before failing.
- **0.1.0.** First published version. Describes the vocabulary Tuff has shipped since 0.1.2, when the Claude Code matrix was corrected to the harness's real event names and canonical names were given precedence over aliases.
