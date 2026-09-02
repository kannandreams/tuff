# Changelog

All notable user-facing changes to Tuff are documented in this file.

The historical entries below were reconstructed from release tags, merged pull requests, and tagged source diffs. Starting after `0.1.3`, changes should be added to `Unreleased` when they merge and moved into a dated version section when a release is prepared.

## [Unreleased]

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

[Unreleased]: https://github.com/kannandreams/tuff/compare/v0.1.8...HEAD
[0.1.8]: https://github.com/kannandreams/tuff/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/kannandreams/tuff/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/kannandreams/tuff/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/kannandreams/tuff/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/kannandreams/tuff/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/kannandreams/tuff/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/kannandreams/tuff/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/kannandreams/tuff/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/kannandreams/tuff/releases/tag/v0.1.0
