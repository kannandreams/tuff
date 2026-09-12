# Tuff Hooks Specification

Version 0.1.0. This document describes the hook vocabulary Tuff implements, how a hook written against it is rendered into each supported harness, and what a second implementation has to do to be compatible. It is generated in part: the tables below come from the same constants Tuff runs on, produced by `tuff hooks spec --json`, and the [machine-readable document](https://github.com/kannandreams/tuff/blob/main/spec/hooks/hooks-spec.json) and its [JSON Schema](https://github.com/kannandreams/tuff/blob/main/spec/hooks/hooks-spec.schema.json) sit beside this file. On tuffcli.dev the same two files are served at `/spec/hooks/hooks-spec.json` and `/spec/hooks/hooks-spec.schema.json`.

The words MUST, SHOULD, and MAY are used as in RFC 2119.

## 1. Scope

A hook is automation a coding harness runs at a defined moment in a session: before a tool call, after a file is saved, when the agent is about to stop. Every harness invented its own event names, its own settings file, and its own idea of what a hook may block. This specification defines one vocabulary above them, a way to declare honestly how well each harness can honour it, and the files an implementation writes so that a hook authored once installs anywhere.

It covers Tuff-standard hooks: a hook declared with a canonical event name. It does not cover native hook fragments, which a harness's own format describes; an implementation passes those through unchanged, and section 6 says only how they are merged.

## 2. Terms

- **Canonical event.** One of the seven event names in section 3. What a hook author writes.
- **Native event.** The name a harness uses for the same moment, such as `PreToolUse` in Claude Code or `preToolUse` in Cursor.
- **Harness.** The coding-agent environment a hook runs in. **Adapter.** The part of an implementation that renders for one harness.
- **Coverage.** How faithfully a harness honours a canonical event: `full`, `partial`, or `unsupported`.
- **Compatibility matrix.** An adapter's declaration of coverage for every canonical event.
- **Registration.** The entry in a harness's settings file that makes it run a command on a native event.
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

**Payload.** The payload descriptor lists fields a harness MAY pass to a handler. In version 0.1.0 every event shares the same two optional fields and none has a required field. An implementation MUST NOT require any payload field, and a handler SHOULD treat every field as absent until it checks.

**Aliases.** A matrix MAY accept other spellings of a canonical event, such as a harness's own native name, so that a hook written against one harness ports to another. Resolution is defined in section 5.

## 4. A Tuff-standard hook

A Tuff-standard hook is declared in a capability manifest with the canonical event, the command to run, and the directory to run it in:

```toml
id = "require-log-summary"
type = "hook"
version = "1.0.0"
description = "Ask for a summary of the session log before the agent finishes."

[hook]
event = "before_finish"
command = "scripts/summarize.sh"
working_directory = "."
```

`working_directory` defaults to `.`, the project root. Any files beside the manifest are the hook's runtime files and are installed with it.

**Rendering.** An implementation renders the declaration into two artifacts per harness. The first is a wrapper script, at `<dir_prefix>/hooks/<id>/run.sh`, of exactly this form, with both values single-quoted for POSIX shells:

```sh
#!/usr/bin/env bash
set -euo pipefail
cd -- '<working_directory>'
exec bash -euo pipefail -c '<command>'
```

The second is a registration in the harness's settings file, under the native event the matrix maps the canonical event to, running `sh <dir_prefix>/hooks/<id>/run.sh` relative to the project root. The wrapper is the contract between the author's command and the harness: the harness runs a fixed, known path, and the author's command and working directory never reach the settings file, which is where shell interpolation would otherwise happen.

**Before rendering** an implementation MUST look the manifest's event up in the target harness's matrix, and MUST behave as section 5 says for the coverage it finds. An implementation MUST NOT execute the hook's command at install time.

## 5. The compatibility model

Every adapter publishes a compatibility matrix: one row per canonical event, in the format of section 8. A row carries the native event name when there is one, the coverage level, the aliases the row answers to, a scope, a caveat, a source, and optional harness version bounds.

**Coverage levels** and what an implementation MUST do at install time:

| Coverage | Meaning | Install behaviour |
|---|---|---|
| `full` | The native event fires at the same moment with at least the canonical blocking effect, for the declared scope. | Install silently. |
| `partial` | The native event is the closest match but differs in scope or blocking, as the row's `scope` and `caveat` say. | Install, and tell the user, quoting the scope and caveat. |
| `unsupported` | The harness has no event for this moment. | Refuse, naming the events the harness does support and the row's caveat when there is one. |

An `unsupported` row MUST NOT name a native event. A `full` or `partial` row MUST.

**Resolution.** Given an event name from a manifest, an implementation MUST first look for the row whose canonical event is that name, and only if none matches look for a row listing it as an alias. This order matters: a harness may use one native name, such as `stop`, for a moment that two canonical events map onto, and the canonical row must win over the alias so that `stop` written in a manifest records `stop` and not `before_finish`.

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
| `pre_tool_use` | `pre_tool_execution` | partial | `pre_tool_execution` | local function tools, Bash, Edit, Write, MCP | Codex hosted tools do not use the local function-tool hook path. ([source](https://learn.chatgpt.com/docs/hooks.md)) |
| `post_tool_use` | `post_tool_execution` | partial | `post_tool_execution` | local function tools, Bash, Edit, Write, MCP | Codex hosted tools do not use the local function-tool hook path. ([source](https://learn.chatgpt.com/docs/hooks.md)) |
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

Hooks install under `.agents/hooks/<id>/`, registered in `.agents/hook.json` (grouped shape) as `sh .agents/hooks/<id>/run.sh`.

| Canonical | Native | Coverage | Aliases | Scope | Caveat |
|---|---|---|---|---|---|
| `before_finish` | `before_finish` | full |  |  |  |
| `after_save` | `after_save` | full |  |  |  |
| `pre_tool_use` | `pre_tool_execution` | partial | `pre_tool_execution` | local function tools, Bash, Edit, Write, MCP | Codex hosted tools do not use the local function-tool hook path. ([source](https://learn.chatgpt.com/docs/hooks.md)) |
| `post_tool_use` | `post_tool_execution` | partial | `post_tool_execution` | local function tools, Bash, Edit, Write, MCP | Codex hosted tools do not use the local function-tool hook path. ([source](https://learn.chatgpt.com/docs/hooks.md)) |
| `session_start` | none | unsupported |  |  | Codex hook.json does not currently define a session-start event. |
| `session_end` | none | unsupported |  |  | Codex hook.json does not currently define a session-end event. |
| `stop` | none | unsupported |  |  | Codex hook.json does not currently define a stop event. |

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

**Settings shapes.** Two shapes of settings file exist among the supported harnesses. In the `grouped` shape, used by Claude Code, Open Agents, and Codex, an event holds groups and a group holds typed entries:

```json
{"hooks": {"PreToolUse": [{"hooks": [{"type": "command", "command": "sh .claude/hooks/<id>/run.sh"}]}]}}
```

In the `flat` shape, used by Cursor, an event holds entries directly and the file declares a `version`:

```json
{"version": 1, "hooks": {"preToolUse": [{"command": "sh .cursor/hooks/<id>/run.sh"}]}}
```

A hook fragment is a document in one of these shapes containing only `hooks`, plus `version` in the flat shape. An implementation MUST refuse a fragment carrying any other top-level key, because a whole settings file offered as a fragment would otherwise be merged key by key into the user's real one.

**Merging.** The settings file belongs to the user. An implementation MUST merge a registration into it rather than replace it, MUST keep every key and every registration it did not write, and MUST NOT add a group an event already holds, so that installing the same hook twice leaves the file unchanged. In the flat shape it MUST ensure `version` is present.

**Recording.** For every registration it writes, an implementation MUST record the settings file path, the native event, the canonical event when the hook was Tuff-standard, the command, and a hash of the entry as written. Those records are what let the implementation later tell whether a registration is still there and unchanged, and whether the hook would port to another harness.

**Removing.** An implementation MUST remove only the registrations it recorded, matched by command within the recorded event, and MUST leave the user's other registrations and other keys alone. A group or event left empty by a removal SHOULD be pruned, so the file reads as if the implementation had never written to it.

## 7. Conformance checklist

An implementation conforms to this version of the specification when all of the following hold. Beside each item is the test in Tuff that pins it, so the claim can be checked against running code.

1. **Vocabulary.** It recognises the seven canonical events of section 3 by their canonical names. *(`tuff-hooks-spec`: `hook_event_display_uses_canonical_name`)*
2. **Resolution order.** It resolves a canonical name before any alias. *(`canonical_name_takes_precedence_over_an_earlier_alias`, `aliases_remain_available_when_no_canonical_name_matches`)*
3. **Matrix completeness.** Every matrix it publishes lists every canonical event exactly once, names a native event on every supported row and on no unsupported row, and states the spec version it targets. *(`tuff-cli`: `every_adapter_matrix_covers_every_canonical_event_exactly_once`)*
4. **Honest coverage.** It refuses an `unsupported` event with the list of supported ones, installs a `partial` event with the caveat shown, and installs a `full` event silently. *(`add_hook_renders_canonical_event_to_native_event`, `add_hook_existing_event_rejected_by_adapter`)*
5. **No execution at install.** It never runs the hook's command while installing, updating, or removing it. *(the wrapper is written, never invoked; `hook_script_preserves_shell_sensitive_values` runs it deliberately, in a test)*
6. **Wrapper script.** It renders the wrapper of section 4 with both values single-quoted, and refuses values containing NUL. *(`hook_script_preserves_shell_sensitive_values`, `hook_script_rejects_nul_bytes`)*
7. **Fragment validation.** It refuses a fragment with keys beyond `hooks` (and `version` in the flat shape). *(`tuff-core`: `a_whole_settings_file_is_refused_as_a_fragment`)*
8. **Idempotent merge.** Merging the same registration twice leaves the settings file byte for byte unchanged, in both shapes. *(`merging_the_same_fragment_twice_does_not_duplicate_the_hook`, `merging_keeps_what_the_user_already_had`)*
9. **Records.** It records path, native event, canonical event, command, and entry hash per registration, and can report whether each is present and unchanged. *(`add_hook_renders_canonical_event_to_native_event` checks the record; `tuff check` reads it)*
10. **Surgical removal.** It removes only its own registrations and prunes what it emptied. *(`removal_takes_out_only_tuff_registrations_in_either_shape`)*
11. **Published matrix.** Its matrices are available as a document valid against the schema of section 8. *(`scripts/check-spec.sh` validates `hooks-spec.json` on every check)*

## 8. Publishing a matrix

The machine-readable form of this specification is a JSON document with three members: `spec_version`, `events`, and `adapters`. Tuff prints its own with `tuff hooks spec --json`; the copy beside this file is regenerated from that command and checked on every change. The JSON Schema beside it describes the document, and an implementation publishing its own matrices SHOULD publish them in the same form so that they can be compared and checked with the same tools.

The `events` member is the table of section 3, one object per canonical event with its blocking scope, the spec version that introduced it, and its payload descriptor. The `adapters` member holds one object per harness: its id and display name, the directory prefix it installs under, the settings file path and shape, the command it registers, the spec version its matrix targets, and the matrix rows.

## 9. Versioning

The specification carries a semantic version, currently 0.1.0, and it moves independently of Tuff's release version:

- A **patch** change reword or clarifies without changing what a conforming implementation does.
- A **minor** change adds a canonical event, a payload field, or an optional row attribute. A hook written against the previous minor still renders the same way.
- A **major** change removes or renames an event, changes an event's blocking scope, or changes the resolution order or the rendering of section 4.

Compatibility matrices are data about harnesses, not part of the vocabulary. A row may change when a harness changes, without moving the spec version; such changes ship with the implementing release and appear in its changelog. Every published matrix states the spec version it targets.

**Compatibility statement.** Each Tuff release implements exactly one version of this specification:

| Tuff releases | Specification |
|---|---|
| 0.1.2 through 0.9.0 | 0.1.0 |

## 10. Changes to this specification

- **0.1.0.** First published version. Describes the vocabulary Tuff has shipped since 0.1.2, when the Claude Code matrix was corrected to the harness's real event names and canonical names were given precedence over aliases.
