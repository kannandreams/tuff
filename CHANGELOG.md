# Changelog

All notable user-facing changes to Tuff are documented in this file.

The historical entries below were reconstructed from release tags, merged pull requests, and tagged source diffs. Starting after `0.1.3`, changes should be added to `Unreleased` when they merge and moved into a dated version section when a release is prepared.

## [Unreleased]

### Added

- The documentation site lists the built-in MCP catalog at [/catalog/](https://tuffcli.dev/catalog/), with each server's install command, the variables it needs, the tools it answers with, and the command it actually runs, filterable by whether it needs a key and by transport. The page is generated from `crates/tuff-core/assets/mcp-catalog.toml` on every build, the same arrangement the changelog page uses, so it cannot promise a server the CLI does not have; the generator also fails the build on an entry that would not resolve. The hand-maintained table on the MCP Servers page is replaced by a link to it, leaving one source of truth.
- Tuff ships a VS Code extension, in `editors/vscode`. It puts the capabilities installed in a project into the editor sidebar with their versions, the agents they were installed for, and whether they have drifted, and runs `diff`, `update`, `check`, and `mcp doctor` on a row. Like the Claude Code plugin it carries no binary and runs the `tuff` on your PATH, and it needs 0.6.0 or newer for the `--json` output that release added. Cursor installs VS Code extensions, so this covers the harnesses that have no plugin surface of their own. The extension versions independently of the CLI and keeps its own changelog.

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

[Unreleased]: https://github.com/kannandreams/tuff/compare/v0.6.0...HEAD
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
