# Changelog

All notable user-facing changes to Tuff are documented in this file.

The historical entries below were reconstructed from release tags, merged pull requests, and tagged source diffs. Starting after `0.1.3`, changes should be added to `Unreleased` when they merge and moved into a dated version section when a release is prepared.

## [Unreleased]

### Added

- **`tuff harness` and `--harness` name what the command group and the flag select.** Tuff says *harness* for the product it installs for, such as Claude Code or Codex, and the word *agent* now means the thing a model runs as. `tuff harness list`, `add`, `remove`, and `set-default` replace `tuff agent`, and `--harness` replaces `--agent` on every command that takes it; `-a` is unchanged. The old names still work until 1.0 and print a note on stderr, except under `--json`. `tuff.config.json` also accepts `harnesses` for its list of registered harnesses, and Tuff still writes `agents`. Messages and help text use the new names, so a script matching `unknown agent` or `registered agent` needs the new wording. The VS Code extension passes `-a`, which every CLI version accepts.
- **MCP servers install for OpenCode through the `opencode` target.** OpenCode reads MCP servers from `opencode.json`, under `mcp`, and not from `.agents/mcp.json`, so a server installed with `-a open-agents` never started in OpenCode. `tuff add mcp <server> -a opencode` now writes the entry into `.opencode/opencode.json`, in OpenCode's own shape: `type` is `local` or `remote`, a local server's program and arguments are one `command` array, its variables sit under `environment`, and a variable is referenced as `{env:VAR}`. The file's other keys keep their order, since OpenCode reads the order of `permission` rules as precedence. `tuff check` reports a hand-edited entry, and `tuff delete` removes the entry, the `mcp` object once it is empty, and the file once only `$schema` is left. The docs no longer say OpenCode reads `.agents/mcp.json`.
- **Policy `mcp` rules are enforced in Codex.** `tuff add` writes them onto the server's table in `.codex/config.toml`: `deny` adds the tool to `disabled_tools`, which removes it from the session, and `ask` sets `approval_mode = "prompt"` on `[mcp_servers.<server>.tools.<tool>]`. Both take exact names, so a rule with `*` is not enforced in Codex and installs only with `--accept-unenforced`. The server must already be in `.codex/config.toml`, or the policy is refused. Codex calls a `prompt` tool without asking when its approval policy is `never` and the sandbox allows full disk access or is off, so Tuff reports these rules as enforced partially. Checked in Codex CLI 0.154.0 in a trusted project: a denied tool was absent from the session, and `codex exec` refused a `prompt` tool. `tuff update` of the server keeps these settings, `tuff check` ignores them when it compares the server's entry and reports them for the policy, and `tuff delete` refuses to remove the server while a policy has settings on it.

### Fixed

- **Codex hooks are written where Codex reads them.** The Codex adapter registered hooks in `.agents/hook.json` under event names such as `pre_tool_execution`, which Codex never reads, so a hook installed for Codex never ran. It now writes `.codex/hooks.json`, in the grouped shape Claude Code uses, with Codex's own events: `session_start` to `SessionStart`, `session_end` to `SessionEnd`, `pre_tool_use` to `PreToolUse`, `post_tool_use` to `PostToolUse`, `stop` to `Stop`, and `before_finish` to `Stop` partially; `after_save` has no Codex event and is refused. The old names stay as aliases, so a manifest that uses one still installs. Checked in Codex CLI 0.154.0, where a `SessionStart` and a `PreToolUse` hook registered this way both ran. Codex loads a project's hooks only in a trusted project and only after they are approved with `/hooks`, and `tuff add` says so. `tuff update <id> -a codex` moves a hook installed earlier, removing its registration from `.agents/hook.json`. The hooks specification's Codex matrix changes with it, without a specification version change, and the Open Agents matrix no longer cites Codex's documentation for events no harness documents.
- **Codex MCP servers are written where Codex reads them.** The Codex adapter registered MCP servers in `.agents/mcp.json`, which Codex does not read, so a server installed with `-a codex` never started in Codex. It now writes `[mcp_servers.<id>]` into `.codex/config.toml`, editing that file in place so the rest of it stays as written: `command` and `args` for a local server, `env_vars` for a variable forwarded under its own name, `url` with `bearer_token_env_var` for an `Authorization: Bearer` header and `env_http_headers` for a plain header variable. A declaration Codex cannot carry, a renamed variable or any other header format, is refused before anything is written. Checked in Codex CLI 0.154.0, where a server declared this way was listed by `codex mcp list` and reached a live session. Codex loads a project's servers only in a trusted project, and `tuff add` says so. `tuff update <id> -a codex` moves a server installed earlier, removing its `.agents/mcp.json` entry unless the `open-agents` target still uses it.

## [0.11.0] - 2026-09-16

### Added

- **`tuff add --accept-unenforced` and `tuff check --strict` for policies.** By default `tuff add` still refuses a policy when a selected agent does not enforce one of its rules. With `--accept-unenforced`, each agent gets the rules it enforces, each rule it does not is printed with the reason and recorded in `tuff.lock`, and an agent that enforces none of the rules still refuses the policy. `tuff check` prints the recorded rules on every run, lists them under `gaps` in `--json`, and exits 0 for them; `tuff check --strict` exits 1 while any are recorded. `tuff update` recomputes the record without the flag. Lockfiles with no recorded rules are unchanged.
- **Policy `command` rules are enforced in Codex.** `tuff add` compiles them into Codex's own command rules in `.codex/rules/tuff.rules`, a file Tuff owns: `deny` becomes `prefix_rule(..., decision = "forbidden")`, `ask` becomes `decision = "prompt"`, and a rule's `reason` becomes the `justification` Codex shows when it refuses. Checked against Codex CLI 0.154.0, which refused `git push --force` in a trusted project. Codex loads project rules only in a trusted project and labels rules experimental. It splits a simple chain joined by `&&`, `||`, `;`, or `|` and checks each command, but checks a script with redirection, substitution, a variable assignment, a wildcard, or control flow as one command, so Tuff reports these rules as enforced partially. Where Codex never asks for approval, as in `codex exec`, a `prompt` rule refuses the command. `read`, `edit`, and `mcp` rules are not enforced in Codex, so a policy with them installs for Codex only with `--accept-unenforced`. `tuff check` reports a rule removed from the file by hand, and `tuff delete` removes the policy's rules and the file once none remain.
- **Policies are enforced in OpenCode, through a new `opencode` target.** After `tuff agent add opencode`, `tuff add` compiles every kind of policy rule into OpenCode's `permission` settings in `.opencode/opencode.json`. OpenCode loads that file after the project's `opencode.json` and applies the last matching rule, and Tuff appends its rules, `ask` before `deny`, so the policy's rules take precedence over the project's own. Tuff keeps the keys, rules, and order already in the file, stops if the file has a rule for the same pattern with a different action, and does not edit `opencode.json`. Command, read, and edit rules are enforced partially: OpenCode checks each parsed command but not `sh -c` or an absolute program path, and file rules cover its read and edit tools but not `grep`, `glob`, or shell commands. MCP rules are enforced fully, and a denied tool is hidden. `opencode run` rejects what an `ask` rule raises, and `opencode --auto` approves it. `opencode` is a policy-only target, so `tuff init` does not register it and `tuff hooks spec` still lists only adapters that take hooks. The new `tuff-adapter-opencode` crate publishes with the others.

### Changed

- **The Claude Code plugin is now version 0.2.0.** Its version had stayed at 0.1.1 since the plugin was first packaged, so Claude Code had no new version to update to, and existing installs could keep an old `tuff-cli-guide` skill, including the `tuff remove` command fixed in 0.10.2. Updating the plugin now brings in the current skill.

## [0.10.2] - 2026-09-14

### Fixed

- **The `tuff-cli-guide` skill no longer tells agents to run commands that do not exist.** The guide that `tuff init` installs for each agent, and that the Claude Code plugin ships, listed `tuff remove <id>`, which Tuff has never had, and wrote git installs as `tuff add <git-url> skill <name>`, which puts the URL where a local path is expected. It now lists `tuff delete` and `tuff untrack`, writes git installs type first (`tuff add skill <git-url> <name>`), and adds the commands it had left out: `tuff scan`, `tuff policy matrix`, `tuff mcp catalog` and `search`, installing a policy, `tuff lock migrate`, and `tuff cache clear`.

### Changed

- The blog has a post on agent policies across harnesses, and the blog index shows each post as a card with a link to read it.

## [0.10.1] - 2026-09-13

### Changed

- **`tuff agent list` names the agents that actually read the Open Agents layout.** The `open-agents` row listed Roo, which shut down in May 2026, and Cline, which reads skills from `.cline/skills/` rather than `.agents/skills/`. Each agent was checked against its own documentation, and the row now lists Codex, Cursor, OpenCode, GitHub Copilot, Gemini CLI, Windsurf, Amp, Goose, and JetBrains Junie. The `tuff-cli-guide` skill that `tuff init` installs lists the same agents, and now also names the `codex` and `cursor` adapters it had left out.
- The documentation site is reorganised. The CLI reference is split into pages by task, `tuff.toml` and `tuff.lock` each have a plain-language page under Start Here, and the landing page shows what a hook and a policy compile to for each agent.

## [0.10.0] - 2026-09-13

### Added

- **The hooks specification is version 0.2.0, tested by a second implementation.** To find out whether the published specification was complete enough to implement, an implementation was written from it alone, by an author with no access to Tuff's source, and a harness installed the same hook with it and with `tuff` for every harness and every event name, comparing what each wrote. The two agreed on every install and refusal, every settings file, idempotency, and removal, and differed in exactly three places the specification had not described: which runtime files are installed and where, how single quotes are escaped in the wrapper, and which object the recorded hash covers. Version 0.2.0 states all three and every other point that implementation had to guess, including that a name matching no row is refused, that formatting of a settings file is not significant, and the removal algorithm step by step. It also adds three requirements to refuse hostile input: an id that is not a relative path of plain names, a listed file that escapes its directory, and a listed file that would replace the wrapper. The harness and that implementation ship as a conformance kit in `spec/hooks/conformance/`, and `mise run check` runs it on every change, so Tuff and its specification cannot drift apart again without a failing check.

- **Policies: rules that narrow what an agent may do, enforced in Claude Code.** A `type = "policy"` manifest lists rules with an effect, `deny` or `ask`, and one subject: a command given as a prefix of its arguments, file paths the agent must not read or edit, or an MCP tool as `server:tool`. Path patterns follow `.gitignore` rules as if the file sat at the project root. Tuff validates a policy when it is loaded and refuses a rule with no subject or two, a command argument containing a space or `*`, a path that is absolute or climbs out of the project, or a misspelt field. There is no `allow` effect and a rule using one is refused, because a policy can come from another team's repository or a pack, and one that could grant permissions could quietly widen what an agent may do in every project that installs it. For Claude Code, `tuff add` compiles each rule into Claude Code's own permission rules in `.claude/settings.json`, such as `Bash(git push --force *)`, `Read(/secrets/**)`, and `mcp__github__delete_*`, merged beside your own rules and settings, and records each one: `tuff check` reports a compiled rule removed by hand, `tuff update` takes out rules a changed policy no longer has, and `tuff delete` removes exactly the policy's rules. Claude Code's own documentation says its command and file rules are not a security boundary, since the same program run by absolute path or through `sh -c` is not matched, so Tuff reports those as enforced partially and prints why at install; MCP rules are enforced fully. `tuff policy matrix` prints, for every agent, how each kind of rule is enforced. Cursor, Codex, and Open Agents enforce no policy rules yet, so `tuff add` of a policy for any of them is refused, naming each rule and the agent that would not enforce it: a policy reported as installed where nothing enforces it would be worse than none.

### Fixed

- **A capability's id can no longer aim an install or a delete outside the project.** A capability id names the directory it installs into and the directory `tuff delete` removes, `<harness>/<kind>s/<id>`, and through 0.9.0 an id was only required to be non-empty. A lockfile entry named `../../../victim` made `tuff delete` remove a directory outside the project, and since a `tuff.lock` can arrive committed in someone else's repository, running `tuff delete` on it was enough. Every id is now required to be a relative path of plain names: nested ids such as `security/security-review` keep working, while `..`, `.`, empty segments, a leading or trailing `/`, backslashes, and NUL are refused. The rule is applied to manifest ids, `--name` overrides for local and git installs, the member ids inside a pack artifact downloaded from a registry, and every name read back from a lockfile. A lockfile holding such a name is refused as corrupt, naming the entry, rather than acted on; remove that entry by hand.
- **A capability's manifest can no longer read or write files outside where it belongs.** Through 0.9.0, an entry in a `tuff.toml` `files` list was joined to the capability directory to read it and to the harness directory to write it, without checking the path. `files = ["../../outside.txt"]` therefore copied a file from beside the capability into the project outside the harness directory, and enough `../` segments aimed the write anywhere the user could write. A listed file that was a symbolic link was followed, so its target's contents were copied into the project. A native hook source added with `--hook-file` followed links the same way. Capabilities are installed from other people's repositories, so a hostile one could have used this to overwrite files in a project or pull a local secret into it. Every `files` entry and a tool's entrypoint must now be a relative path inside the capability directory with no `..`, no leading `/`, and no symbolic link anywhere along it, and must name a regular file; native hook sources refuse links; and as a second line of defence nothing is written outside the install directory whatever planned it. The install is refused before anything is written. Skill directories without a manifest and capability packs already refused both and are unchanged. If you have installed capabilities from sources you do not control, review their `tuff.toml` `files` lists for `..` or links.
- **A hook's listed files can no longer replace the script Tuff runs.** The registration Tuff writes runs `run.sh` in the hook directory, and the install note names the command it wraps. A hook that listed its own `run.sh`, or `src/run.sh`, which installs to the same place, overwrote that wrapper, so the harness ran a script the note never mentioned. That is now refused before anything is written, with a hint to rename the file.
- When a hook's event is refused, the message now lists the event names a manifest can actually use. It used to list the harness's native names, so Cursor's refusal suggested `preToolUse` in the same message that had just refused `preToolUse`, and it named `stop` twice because two events render to it. It now reads, for Cursor, `pre_tool_use (renders as preToolUse)`, and a native name appears as a usable name only where that harness accepts it as an alias. An event a harness knows but cannot support, such as `after_save` for Claude Code, used to be refused with only its caveat, and now lists the usable names too.
- **Two listed files that would install under the same path are refused.** One leading `src/` is removed from each listed file's installed path, so `check.sh` and `src/check.sh` both install as `check.sh`. Tuff used to write both, keep whichever came second, and report the file installed twice. Such a manifest is now refused before anything is written, naming both files. The same file listed twice, with or without a leading `./`, still installs once.
- **Deleting a capability whose harness settings file is not valid JSON no longer loses its files.** `tuff delete` removed the capability's directories first and only then read the settings file to take out its hook registrations, so a corrupt settings file made the delete fail after the files were gone, with the capability still recorded in `tuff.lock`. It now updates the settings file first, so the same failure leaves the files in place and the capability tracked, and a retry after fixing the file completes.

## [0.9.0] - 2026-09-12

### Added

- **The hooks specification is published.** The vocabulary Tuff renders hooks from, seven canonical events with what each may block, and every harness's compatibility matrix are now a versioned document at [tuffcli.dev/spec/hooks/](https://tuffcli.dev/spec/hooks/) and under `spec/hooks/` in the repository, with a conformance checklist for anything else that wants to implement it and a JSON Schema for the machine-readable form. It is generated, not written: `tuff hooks spec` prints the specification the binary implements, for every harness whether or not the project registers it, and `--json` prints the document the published files are made from. A repository check fails when they drift, so the published tables cannot say something `tuff add` does not do. The hand-kept event tables on the Hooks and Harness Adapters pages, one of which had already drifted, are replaced by links to it. The specification is version 0.1.0 and describes what Tuff has shipped since 0.1.2.
- **`tuff outdated` says when a git install could follow a release instead of a commit.** A capability added from a repository without an `@` follows HEAD, and until now nothing told you when that repository started tagging releases; RFC-101 had left open where that hint should live, because putting it on `tuff add` would cost every untagged install a `git ls-remote`. It lives on `outdated`, where the listing already happens: an untagged git row whose repository publishes releases gets a note on standard error under the table naming the newest release and the `tuff update <id>@^<major>.<minor>` that pins it, once per capability however many agents it is installed for. `--json` carries the newest release as `latest_release` on every git row. The note is on standard error in both modes, so the JSON stays clean.

### Changed

- The four harness adapters now share one implementation of hook-settings handling, in `tuff-core`. Each adapter used to carry its own copy of the code that merges a hook registration into the harness's settings file and takes it out again, so a fix had to be made four times at once and the copies had drifted in wording. An adapter now declares only what differs: the shape of its settings file, its paths, its event matrix, and how to recognise a project that uses it. Nothing the adapters write has changed; the only visible difference is that the messages for a malformed `--hook-file` fragment read the same for every harness.
- `tuff outdated` checks a commit-following git install with one `git ls-remote` instead of a clone. The listing names HEAD and every tag in one round trip, so a `sha` entry never clones, and a `declared` entry clones only when HEAD has moved, which is when the version it declares now has to be read. Each capability is also checked once rather than once per agent it is installed for, which previously cloned a capability installed for two agents twice.

## [0.8.0] - 2026-09-09

### Changed

- **`tuff.lock` is JSON, lockfile schema version 3.** The file keeps its name and its rows keep their fields, names, and order; only the syntax changes, from TOML to JSON in the layout `JSON.stringify(value, null, 2)` produces: two-space indentation, one array element per line, a trailing newline. The reason is formatters. Repositories run pre-commit hooks and editor formatters over their JSON files, and a `.lock` that is not JSON was rewritten into something Tuff could not read; asking every project to exclude the file was the wrong fix. The chosen layout is the one npm's `package-lock.json` uses and the one `jq`, Python, VS Code's formatter, and Prettier's `json-stringify` parser all emit, so those leave the file byte for byte unchanged; a test proves the golden fixture is a fixed point of both the writer and a formatter. Version 1 and version 2 files are read transparently by every command; read-only commands never rewrite them, the first mutating command writes version 3, and `tuff lock migrate` does only the rewrite. A file whose syntax does not match its version, and a lockfile from a newer Tuff, are refused with a message saying so rather than a parse error. Tuff 0.7 and earlier cannot read a version 3 lockfile, so a project that migrates needs everyone on 0.8 or newer, the VS Code extension included; the extension itself reads the lockfile only through the CLI and needs no change. Absent optional fields on an MCP server entry, such as `url` for a stdio server, are omitted rather than written as `null`; the installed `server.toml` records are unchanged because TOML already omitted them.
- The VS Code extension shipped 0.2.0 through 0.2.3 in this window: a Get Started walkthrough, Add from Git URL, and the Marketplace listing with its recording. The extension versions independently and keeps its own changelog under `editors/vscode`; nothing in the CLI changed for it.
- The documentation site's dependencies moved past four npm advisories against astro, sharp, svgo, and js-yaml. Site only; nothing in the CLI changed.

## [0.7.0] - 2026-09-06

### Added

- **`tuff scan` finds capabilities Tuff is not tracking.** Every inventory command reads the lockfile, so a skill written by hand in `.claude/skills/` was invisible to `list`, `status`, and `check` no matter how long it had been there; `tuff add <path>` could adopt one in place, which made discovery, not adoption, the missing half. `tuff scan` reads `.claude`, `.cursor`, and `.agents`, and reports every capability directory in them with its id, kind, declared version, harness, and whether Tuff already tracks it. `--adopt` tracks them, either all at once or the paths you name, and each one goes through the same code path `tuff add` uses, so scanning can never track something adding would refuse. Adoption is in place: nothing is moved, copied, or rewritten. Two directories declaring the same id are reported as a conflict rather than one being adopted silently, and a directory missing something Tuff needs — a `[hook]` section, a `[server]` section — is reported with the reason instead of failing halfway through the adopt. Scanning itself changes nothing and works before `tuff init` has run; `--adopt` writes to the lockfile, so it asks you to init first. `--json` carries the same rows with the reason and an `initialized` flag.
- **`tuff mcp catalog` lists the built-in catalog.** `tuff mcp search` reaches the registry and `tuff add mcp <id>` installs by name, but until now nothing could show what the ids are, so finding one meant reading the website or the source. The command prints every entry compiled into your binary with its version, transport, and the environment variables it expects you to export, and `--json` adds the full invocation the harness would run and the tools the entry advertises. Every row is resolved through the same code path `tuff add mcp` uses, so the listing can never offer a server the installer would refuse. It reaches no network and needs no project.
- Tuff ships a VS Code extension, in `editors/vscode`. It puts the capabilities installed in a project into the editor sidebar with their versions, the agents they were installed for, and whether they have drifted, and runs `diff`, `update`, `check`, and `mcp doctor` on a row. Like the Claude Code plugin it carries no binary and runs the `tuff` on your PATH, and it needs 0.6.0 or newer for the `--json` output that release added. Cursor installs VS Code extensions, so this covers the harnesses that have no plugin surface of their own. The extension versions independently of the CLI and keeps its own changelog.
- The site lists the built-in MCP catalog at [/mcp-catalog/](https://tuffcli.dev/mcp-catalog/): every server's install command, the variables it needs, the tools it answers with, and the command it actually runs, searchable and filterable by whether a key is needed and by transport. It is a standalone browse page rather than a documentation page, so the card grid gets the full page width instead of a column between two sidebars, and it carries the site navigation back into the docs. The listing is generated from `crates/tuff-core/assets/mcp-catalog.toml` on every build, the same arrangement the changelog page uses, so it cannot promise a server the CLI does not have, and the generator fails the build on an entry that would not resolve. The hand-maintained table on the MCP Servers page is replaced by a link to it, leaving one source of truth.

### Changed

- Reworded the built-in catalog's `everything` and `playwright` descriptions. Both used a double hyphen as a dash and one wrapped a command in backticks, which read as markup wherever the description is shown: the catalog page, `tuff list`, and the tracked `server.toml`. The entry versions are unchanged, so no installed server reports itself outdated over wording.

### Fixed

- A capability already sitting in `.cursor/` is now adopted where it is, for Cursor. `.claude` and `.agents` were recognised as harness layouts and `.cursor` was not, so `tuff add .cursor/skills/<name>` copied the files into the Open Agents layout and recorded the wrong harness. Reading the harness from the path also no longer depends on the capability being exactly one level deep, so a skill inside a grouping directory, such as `.claude/skills/security/security-review/`, is attributed to Claude rather than to Open Agents.

## [0.6.0] - 2026-09-05

### Added

- **Git-sourced capabilities can be installed at a release.** `tuff add skill <repo> <name>@1.2.0` installs exactly that release, and `<name>@^1.2` the newest release in the range. Releases are read from the repository's tags without cloning: `v1.4.0` or `1.4.0` for the whole repository, `<name>/v1.4.0` or `<name>-v1.4.0` for one capability in a monorepo, and capability-scoped tags take precedence whenever any exist, so a repository-wide tag is never mistaken for a release of one skill inside it. The chosen tag is then cloned shallowly. A requirement nothing satisfies fails before anything is cloned and lists the releases that exist; a repository with no release tags says how to tag one. The lockfile keeps pinning the commit and records the tag and the requirement beside it, with the entry's version now the release's and `version_scheme = "semver"`, using the fields lockfile v2 reserved for this. Installing without `@` is unchanged.
- **The lifecycle verbs understand the pin.** `tuff update` moves a release-pinned capability to the newest release its requirement allows, never to the latest commit, and says when that release is already installed. `tuff update <id>@<requirement>` records a new requirement and moves, which is how an exact pin is lifted, and `--check` previews the release and the claimed size of the change, as in `to 1.4.0 (minor)`. `tuff outdated` compares against the newest release the repository has, with one `ls-remote` and no clone, and shows `outdated (minor)`; in `--json` the size is a separate `change` key. `tuff diff <id> --upstream` compares against the newest allowed release rather than the latest commit, the same content `update` would install, and `tuff diff <id>@<requirement> --upstream` previews a different requirement before `update` applies it; a note on standard error names the release and the JSON carries it as `upstream`.
- **A release tag that moved or vanished upstream is reported rather than trusted**, as a pack's registry tag already was. `tuff outdated` verifies the installed tag in the same `ls-remote`, comparing the commit it names now with the one recorded at install: a mismatch reads `repointed` and a deleted tag `tag missing`, both winning over `outdated` while `LATEST` still shows the newest release. `tuff update` on a repointed entry refuses to call it up to date, previews the replacement with `--check`, and replaces the install with `--force`. The lockfile pins the commit, so the install itself was never affected; what changed is what the version claims to be.
- **A git install that no release tag chose records the version the source declares for itself:** `version` in `tuff.toml`, else `version:` or `metadata.version:` in the `SKILL.md` frontmatter, where the Agent Skills specification puts it. The lockfile marks it `version_scheme = "declared"`; a source declaring nothing still records the commit SHA with `version_scheme = "sha"`. A declared version may not move when the content does, so `tuff list` and `tuff outdated` show it as `1.2.0 (declared)` for a git install, `outdated` compares the version declared then with the one declared now and names the claimed size of the change, and a commit that moved without a version bump still reads `outdated`. A local skill without a `tuff.toml` also takes its frontmatter version instead of `0.1.0`.
- **`tuff list --json` and `tuff outdated --json`** print their rows as JSON arrays, and `tuff diff <id> --json` is a shorthand for `--format json`. The keys `type`, `target`, and `status` are spelled as in `tuff check --json`, so a script or an editor integration reads every inventory command the same way. Rows carry `version_scheme`; where the `outdated` table shows `—`, the JSON carries `null`; status strings are emitted plain, without the terminal colouring the tables use.
- **`linear` and `context7` join the built-in MCP catalog.** Both are remote servers that authenticate with an `Authorization: Bearer` header, which the catalog could not express until `[server.headers]` existed; each entry now declares the header as a reference to `LINEAR_API_KEY` or `CONTEXT7_API_KEY`, and `tuff add mcp linear` writes the right dialect for every selected harness with the key still in your environment. Linear's interactive OAuth flow is not used, because a config file can carry a variable reference and not a login. Context7's key is recommended rather than required by the vendor, but the catalog has no optional headers, so the entry asks for one; the keyless stdio form remains available from the registry as `io.github.upstash/context7`.

### Fixed

- A git install with a `tuff.toml` recorded the commit SHA over the manifest's own version, and `tuff update` on such a source synthesized a skill manifest instead of reading the `tuff.toml` as `add` does, so a tool or workflow from git would have updated as a skill. Both paths now share one helper.

## [0.5.0] - 2026-09-03

### Added

- The `tuff-cli-guide` skill now reaches the harness you actually run. `tuff init` recorded it against `open-agents` and nothing else, and Claude Code reads `.claude/` while Cursor reads `.cursor/`, so the reference that teaches an agent to drive Tuff was invisible in the session where it was needed. `init` now detects the harnesses a project already contains and emits the guide into each one's layout: a `.claude/` directory or a `CLAUDE.md` file registers `claude`, a `.cursor/` directory registers `cursor`. Codex is deliberately not detected, because it writes the same `.agents/` root `open-agents` already covers and its detector matches the directory `init` itself creates.
- A capability that is already installed can now be emitted for a harness it was not installed for. `tuff add .agents/skills/release-checklist --agent claude` records the new target and writes the harness-native output, where it previously refused with `already in the 'open-agents' agent layout`. Nothing else could do it either: `tuff agent add` registered a harness without backfilling, and `tuff update -a claude` reported the capability was not installed for that agent. The recorded source, version, and description are preserved, so adding a harness to a capability installed from Git keeps its repository and resolved revision. A target that is already recorded is still refused, since re-emitting one is what `tuff update` is for.
- Tuff is installable as a Claude Code plugin, which is the same guide reaching a session before any project has adopted Tuff. `claude plugin marketplace add kannandreams/tuff` followed by `claude plugin install tuff@tuff` installs it machine-wide, and `--scope project` records it for everyone working in the repository. The plugin carries no binary: it expects `tuff` on PATH and tells the agent how to install one when the command is missing.

### Changed

- The `tuff-cli-guide` skill no longer opens by asserting that Tuff is installed in the current project, which was untrue wherever the guide arrived before `tuff init` did. It now tells the agent to check `tuff --version`, to ask before installing anything, and to install with `uv tool install tuffcli`, naming Homebrew and Cargo as the fallbacks. The curl installer is deliberately absent from the agent-facing guide: it targets `/usr/local/bin` and escalates with `sudo`, which stalls an agent waiting on a password nobody is there to type. Bare `pip install` is named only as a thing to avoid, since most systems refuse it as an externally-managed environment and a virtual environment install is not on PATH afterwards.

## [0.4.0] - 2026-09-03

### Added

- Added `tuff mcp search <query>`, which searches the official MCP registry, and taught `tuff add mcp <name>` to install from it when the name is not a built-in catalog id. The catalog's twelve curated entries stay the shortcut; the registry's thousands are now reachable by name. Tuff assembles the launch command from the entry's package type (`npm` under `npx`, `pypi` under `uvx`, `oci` under `docker`, `nuget` under `dnx`), pins the version the registry lists, and records environment variables as references, never values. An entry it cannot express exactly is refused with the reason rather than installed approximately. `tuff outdated` and `tuff update` re-resolve a registry install against its registry, and `--registry` points any of it at a self-hosted one.
- MCP servers reached over HTTP can now declare the auth header they need, which is what most remote servers require and what Tuff previously had no way to express. `[server.headers]` takes the same `{ from_env = "NAME" }` references `[server.env]` already takes, plus an optional `format = "Bearer {}"` for the common case where the header wraps the token, so a manifest still has no field a literal secret can occupy. Each harness gets its own dialect: Claude Code, Codex, and Open Agents expand `${VAR}`, Cursor `${env:VAR}`. `tuff check` catches a header edited by hand, and the post-install reminder names header variables alongside environment ones.
- `tuff mcp doctor` now probes HTTP servers instead of reporting `unsupported transport`, doing the same `initialize`, `notifications/initialized`, `tools/list` handshake it does over stdio and reporting the real tool count. It accepts either response shape a server may choose, a plain JSON body or an SSE stream, carries the session id the server issues on initialize, and echoes the protocol version the server negotiated. Two statuses are new: `unauthorized` when the server answers 401 or 403, kept separate from `protocol error` because the fix is to check the token rather than the config, and `unreachable` for a DNS, TLS, or connection failure. A variable a header references but your shell does not export is reported as `missing env` before any request leaves the machine. Header values are read from the environment at the moment of the request, so doctor checks exactly what the harness will send; there is deliberately no `--header` flag.
- Remote servers in the MCP registry that authenticate with a header now install instead of being refused. A required header the entry documents as `Bearer {vendor_api_key}` becomes `Authorization = { from_env = "VENDOR_API_KEY", format = "Bearer {}" }`; a header the entry names without saying how to build its value becomes a reference to a variable holding the whole value, prefix included, because guessing a `Bearer ` nobody wrote down would be right often and wrong silently. Optional headers are left out and named at install time, since requiring a variable the server does not require would report a working server as `missing env`. A header the entry documents as a literal, such as `Accept: application/json`, is still refused: a manifest has no field a literal value can occupy.

### Fixed

- Registry entries offering only the superseded `sse` transport were installed as though they spoke Streamable HTTP, writing a config no harness could use. They are now refused with that reason, and an entry publishing both transports installs the `streamable-http` one rather than whichever was listed first.
- Cursor was written a `"type": "http"` key on remote MCP server entries, which its config format does not use; it distinguishes a remote server from a stdio one by `url` versus `command`. Tuff no longer emits it for Cursor. Any HTTP server already installed for Cursor will show as `modified` on the next `tuff check` until it is reinstalled.

## [0.3.0] - 2026-09-02

### Added

- Every command now reports failures by kind, so the exit code and the `--json` envelope say what kind of problem it is: a mistyped flag or argument exits `2`, while a missing capability, a refused overwrite, local changes, an unreachable source, an unreadable file, and an unsupported request all exit `1` with a distinct `kind`. Advice that used to be appended to a message with a semicolon, such as `run 'tuff agent list'` or `use --force`, now prints on its own `hint:` line.
- Pack commands now report failures by kind: an artifact that will not parse reads as corrupt, a refused overwrite as refused, local changes as drift, an unreachable registry as a source failure, and a mistyped flag exits 2. Advice that used to be appended to a message with a semicolon now prints on its own `hint:` line.
- Errors now carry a kind, and commands use it to choose an exit code: `0` success, `1` a failed operation, `2` a command called wrongly, `70` a bug in Tuff. Messages that suggested a next step now print it as a separate `hint:` line, and a `--json` invocation reports failures as one JSON line on stderr with `kind`, `message`, and `hint` fields rather than prose.

### Fixed

- Fixed `list`, `status`, `outdated`, and `check` reporting a corrupt or unreadable `tuff.lock` as though nothing were installed. They now fail and say the lockfile could not be read. A global lockfile that simply does not exist is still not an error.

## [0.2.0] - 2026-09-02

### Changed

- **Lockfile schema version 2.** A capability's origin is now one `[capabilities.source]` table with a `kind` of `local`, `git`, `catalog`, or `pack`, replacing the `source`, `repository`, `source_path`, and `resolved_ref` columns and the optional `pack` table. Every row gains `version_scheme` (`declared`, `sha`, or `semver`), reserved so release-tag resolution can land later without another schema change. `emittedFiles` and `scope`, which were never persisted, are removed. Version 1 files written by 0.1.x are read transparently by every command; read-only commands never rewrite them, the first mutating command writes version 2, and `tuff lock migrate` does only the rewrite. A lockfile from a newer Tuff is refused by version number rather than failing as a parse error. Version 1 stays readable throughout 0.2.x.

### Added

- Added `tuff lock migrate`, which rewrites `tuff.lock` in the current schema and changes nothing else.
- Added pack updates: `tuff update <member>` on a capability installed by `tuff add pack` now moves the whole pack forward. With a registry on record it resolves the newest semver tag, pulls it, and applies it; `--pack <artifact>` applies a pulled file instead, for offline use or a pack installed without `--reference`. Members the new release drops are removed, new members are installed, shared hook and MCP registrations follow, and `--check` previews all of it. Local edits block the update unless `--force` is given. `tuff update` gained `--plain-http` and `--ca-file`, matching `tuff outdated`.
- Added detection of a pack tag silently repointed to different content. `tuff outdated` now resolves each installed pack's tag and compares its digest with the one recorded at install; a mismatch reads `repointed` and a deleted tag reads `tag missing`, both taking precedence over `outdated`. `tuff update` on such a pack refuses to report it as up to date and explains that `--force` replaces the installed release with what the tag serves now. One manifest fetch per pack per run, no artifact download; registry lookups are also no longer repeated for every member and harness of the same pack.

### Fixed

- Fixed a project-scoped install landing in the global lockfile when `XDG_STATE_HOME` was set on a machine that had used `--global`. The lockfile path was inferred from the directory and treated the project root as a home directory; the scope is now passed explicitly everywhere.
- Fixed `tuff add pack` refusing to install into any project that already had a tool, workflow, or MCP server. The generated capability index those give a project is tracked, but the pack install treated the staged copy of it as an untracked file it must not overwrite.

### Improved

- The documentation site now renders `CHANGELOG.md` as a changelog page, generated at build time so there is one copy that cannot drift, and the release checklist lives in CONTRIBUTING.md.
- Rewrote the MCP Servers reference page: explained that the built-in catalog is a list of launch declarations embedded in the binary rather than server code, and replaced the manifest example's archived npm package with the catalog's verified Docker entry.
- Updated the documentation site's build dependencies for four advisories published against `fast-uri`; nothing in Tuff itself uses the package.
- Added a blog to the documentation site at `/blog/`, linked from the landing page and the docs header, with a first post walking through the MCP server capability end to end on the catalog's `everything` server. Landing-page navigation links now highlight as a dark panel on hover.

## [0.1.8] - 2026-09-01

### Added

- Added `mcp-server` as a capability type. One `[server]` declaration in `tuff.toml` becomes the correct `mcpServers` entry in every selected harness's config, in that harness's dialect, plus a tracked `server.toml`, so `list`, `check`, `diff`, `update`, `delete`, and `outdated` all work on it unchanged. Secrets are references only: `[server.env]` accepts `{ from_env = "NAME" }` and rejects a literal value at parse time. An existing `mcpServers` entry that Tuff does not track is refused before any file is written.
- Added `tuff add mcp <id>...` with a built-in catalog of 12 verified servers: `filesystem`, `memory`, `github`, `fetch`, `git`, `time`, `sequentialthinking`, `everything`, `brave-search`, `notion`, `playwright`, and `sentry`. Catalog installs record `source = "catalog"` and re-resolve against the embedded catalog on `update` and `outdated` instead of cloning. At a terminal, `tuff add mcp` asks once per required environment variable whether to use a different variable name than the catalog default; `--yes` or a non-terminal stdin skips the prompt.
- Added `tuff mcp doctor`, which spawns each installed MCP server, completes the `initialize` handshake, and lists its tools, so a mistyped command, a missing package, or an unset token is reported instead of failing silently inside the harness. Supports `--agent`, `--global`, `--json`, `--timeout`, and `--ignore-failures`, and exits non-zero on any unhealthy server. Stdio transport only; `http` reports `unsupported transport`.
- Added drift detection for managed MCP config entries. Registering a server (or an MCP-native tool) records a baseline hash of its `mcpServers` entry, so `tuff check` fails on a hand-edited or removed entry, `tuff list` shows it as modified, `tuff delete` requires `--force`, and `tuff update --force` restores a tampered catalog entry. Entries installed before this release are unchecked until reinstalled.
- Added a generated per-harness capability index: a `tuff-capabilities` skill listing every installed tool, workflow, and MCP server with its exact invocation. It is regenerated on every install, update, and delete, including `tuff add pack`, and removed once a harness has nothing left to list.
- Added `implementation`, `parameters`, `workflow`, and `server` fields to capability lock entries, cached at install time so the index and `update` can see a capability's shape after its manifest is gone. Existing lockfiles parse unchanged.

### Fixed

- Fixed `tuff add pack` staging installs in a temporary directory that had no `tuff.config.json`, so any per-harness step there silently saw zero configured agents.
- Fixed the wire framing in the `mcp-server-tool` example server, which used LSP-style `Content-Length` headers instead of the newline-delimited JSON-RPC that real MCP servers speak.
- Fixed the lockfile writer hardcoding `source = "git"`; it now persists the recorded source type.

### Improved

- Refreshed the landing page: two-column hero with the terminal demo beside the copy, a `brew` install tab, a strip of supported harnesses, and fixes for desktop horizontal overflow, a squeezed mobile capability grid, unstyled footer links, and a dark band under the footer.

## [0.1.7] - 2026-08-30

### Added

- Added `--reference` to `tuff add pack`, recording the OCI reference a pack was pulled from so `tuff outdated` can check the registry for a newer version. `tuff outdated` gained `--plain-http` and `--ca-file`, matching `tuff pack push`/`pull`, for checking a self-hosted registry.
- Added a CI cache for Rust dependencies (`Swatinem/rust-cache`). `Tuff Check` dropped from about 8.5 minutes to under 2 on a warm cache; nothing else changed.
- Added `gitleaks` as a pre-commit hook, scanning staged changes for generic secrets before a commit exists.

### Fixed

- Stopped `tuff outdated` from reporting `up to date` for a capability it had not checked — anything installed from a pack, or from a local path. It now reports `not checked`, styled to make clear it is not a clean bill of health.

### Improved

- Added credential file patterns to `.gitignore` (keys, certificates, dotenv files) as a preventative measure; no leak was found.
- Defined "capability pack" and "Tuff pack" once, on the page that owns the concept, and used each consistently: the vendor name where the artifact is being distinguished from a container image, the category name everywhere else.

## [0.1.6] - 2026-08-29

### Fixed

- Stopped `tuff add` from registering a hook twice when the same capability or pack is installed over an existing install. Every adapter appended hook groups to the harness settings file unconditionally, so each re-add left another identical entry behind and the harness ran the hook once per copy. Affects the Claude, Codex, Cursor, and Open Agents adapters.

## [0.1.5] - 2026-08-27

### Added

- Added `tuff pack build --name <name>` for packaging accepted project-scoped capabilities directly from `tuff.lock`, with capability selectors, workflow dependency expansion, version and agent overrides, and a `tuff-dist/` default output.
- Added `tuff pack init <name> --from-project` for reusable ID-based definitions under `tuff-packs/` without copying capability sources.

### Improved

- Made project pack builds verify selected installed files and reconstructed sources against accepted lockfile baselines before writing an artifact, with actionable `tuff update` guidance.

### Fixed

- Deduplicated Git capability discovery paths so a directly selected nested capability is not reported as ambiguous.
- Kept project pack builds read-only for `tuff.config.json` when the default-agent configuration is absent.

## [0.1.4] - 2026-08-27

### Improved

- Made the “Explore Tuff Packs” landing-page link a primary call to action.
- Standardized public pack documentation around the `crm-integration` example and linked the beginner-focused [Tuff Pack examples repository](https://github.com/kannandreams/tuff-pack-examples) from the CLI and capability-pack documentation.

## [0.1.3] - 2026-08-25

### Added

- Added capability packs as deterministic, versioned bundles of skills, tools, hooks, and workflows.
- Added `tuff pack init`, `check`, `build`, `inspect`, and `verify` for authoring and validating `.tuffpack` artifacts.
- Added atomic project installation with `tuff add pack`, including per-capability pack provenance in `tuff.lock`.
- Added `tuff pack push` and `pull` for OCI-compatible registries, with tag and digest references, Docker and Podman credential discovery, private CA support, and explicit opt-in plain HTTP for local registries.
- Added `tuff pack extract` for producing a verified harness-native runtime tree without creating project lockfile state.
- Added an Amazon ECR and Docker BuildKit guide showing digest-pinned publication, pull, extraction, and container-image delivery.

### Improved

- Made pack builds reproducible through canonical ordering and deterministic metadata, allowing identical inputs to produce identical artifact digests.
- Added safe OCI tag behavior: identical pushes are idempotent, while moving an existing tag requires an explicit `--force`.
- Enforced Conventional Commit subjects locally and in pull requests.

## [0.1.2] - 2026-08-21

### Improved

- Added canonical hook event definitions and aliases shared by adapters, with canonical names taking precedence.
- Made terminal color output respect TTY detection and improved global and name-filtered validation behavior.
- Hardened release creation so missing checksum assets fail the workflow.

### Fixed

- Updated Claude hook rendering to use the documented native events: `SessionStart`, `SessionEnd`, `PreToolUse`, `PostToolUse`, and `Stop`.
- Corrected Cursor stop-event resolution.
- Prevented malformed MCP configuration from modifying files or partially installing a capability.
- Made hook shell wrappers and workflow TOML serialization safe for special characters.
- Corrected the package version after the `v0.1.1` tag shipped GitHub archives whose binaries still reported `0.1.0`.

## [0.1.1] - 2026-08-20

> Distribution note: this tag produced GitHub archives, but the Cargo package version was not bumped. Those binaries report `0.1.0`, the PyPI workflow failed, and no `tuffcli==0.1.1` package exists. Use `0.1.2` or later.

### Improved

- Adopted mise with pinned Rust, Node.js, Python, Perl, pre-commit, and documentation tooling for reproducible development.
- Hardened website dependency installation and upgraded Astro and Starlight.
- Separated the user-facing README from repository guidance and added clearer contribution, conduct, and agent-maintainer documentation.
- Added pre-commit branch-name validation and improved installation-script behavior.

### Fixed

- Corrected documentation table styling and ensured `rustfmt` and Clippy are installed with the pinned Rust toolchain.
- Made GitHub release creation fail when expected checksum assets are missing.

## [0.1.0] - 2026-07-26

### Added

- Released the Rust-based `tuff` CLI for managing project-owned agent skills, tools, hooks, and workflows.
- Added local and Git-backed capability installation, project and global scopes, and adapters for Open Agents, Claude, Codex, and Cursor.
- Added `tuff.lock` lifecycle tracking with cached baselines, drift detection, upstream comparison, diff, update, validation, delete, and untrack workflows.
- Added capability index and project report generation, hook portability checks, and agent registration and default selection.
- Added GitHub release archives for macOS arm64, macOS x86_64, and Linux x86_64, plus installation through PyPI, crates.io, Homebrew, and the install script.
- Added the Astro and Starlight documentation site and the initial Tuff landing page.

### Improved

- Renamed the project from Coral to Tuff and standardized the CLI, manifests, documentation, and adapter terminology.
- Refined adapter and renderer contracts so harness-specific output remains isolated behind dedicated adapter crates.
- Added repository validation, integration tests, release automation, and reproducible Cargo builds.

[Unreleased]: https://github.com/kannandreams/tuff/compare/v0.11.0...HEAD
[0.11.0]: https://github.com/kannandreams/tuff/compare/v0.10.2...v0.11.0
[0.10.2]: https://github.com/kannandreams/tuff/compare/v0.10.1...v0.10.2
[0.10.1]: https://github.com/kannandreams/tuff/compare/v0.10.0...v0.10.1
[0.10.0]: https://github.com/kannandreams/tuff/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/kannandreams/tuff/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/kannandreams/tuff/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/kannandreams/tuff/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/kannandreams/tuff/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/kannandreams/tuff/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/kannandreams/tuff/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/kannandreams/tuff/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/kannandreams/tuff/compare/v0.1.8...v0.2.0
[0.1.8]: https://github.com/kannandreams/tuff/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/kannandreams/tuff/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/kannandreams/tuff/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/kannandreams/tuff/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/kannandreams/tuff/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/kannandreams/tuff/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/kannandreams/tuff/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/kannandreams/tuff/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/kannandreams/tuff/releases/tag/v0.1.0
