# Contributing to Tuff

Thanks for taking the time to improve Tuff.

Tuff is a Rust CLI for managing project-owned agent capabilities across coding
agent harnesses. The CLI surface, lockfile format, adapter behavior, and
documentation are still evolving. Contributions are welcome, but keeping the
project focused is important while those foundations settle.

## Before you start

For a bug fix, documentation change, test improvement, or other small and
clearly scoped change, you can open a pull request directly.

For a new command, capability type, adapter, lockfile change, public API, or
other substantial feature, please open an issue first. Describe the problem,
the user or workflow it affects, and a possible direction. Wait for the
maintainers to confirm the scope before investing in a large implementation.

An issue or feature discussion does not guarantee that the change will be
accepted. Tuff may be deliberately kept small, and maintainers may suggest a
different design, defer the work, or close a proposal that does not fit the
current direction.

## Ways to contribute

You can help by:

- Reporting reproducible CLI bugs.
- Sharing real-world agent-harness workflows and use cases.
- Suggesting focused improvements to commands, output, or error messages.
- Improving the documentation and examples.
- Adding or strengthening tests.
- Testing Tuff on another operating system, shell, or harness adapter.
- Proposing compatibility improvements for capability formats and lockfiles.
- Adding an MCP server to the built-in catalog (see [MCP catalog entries](#mcp-catalog-entries)).

## Bug reports

Before opening an issue, search existing issues and check that you are using a
recent build. Include enough information for someone else to reproduce the
problem:

- Tuff version (`tuff --version`).
- Operating system and architecture.
- Rust, Python, or Node version when relevant.
- The command or workflow you ran.
- A minimal manifest, capability, or repository layout when relevant.
- Expected behavior and actual behavior.
- Relevant output, errors, or screenshots.

Please remove secrets and private repository data before posting logs or
configuration files.

## Local setup

Install [mise](https://mise.jdx.dev/getting-started.html), then prepare the complete development environment from the repository root:

```sh
mise run setup
mise run cli -- --help
```

Mise installs the pinned Rust, Node.js, Python, Perl, `pre-commit`, and terminal-screenshot tooling declared in `mise.toml`. It also fetches Cargo dependencies, installs the website dependencies with npm, and enables the repository Git hook.

The host still needs Git, a C compiler, and `make`; these are required before mise can clone the repository or compile Tuff's vendored native dependencies.

If you do not use mise, install the versions declared in `mise.toml` and run the underlying setup commands directly:

```sh
cargo fetch --locked
npm --prefix website ci
pre-commit install
```

## Branch names

Tuff validates work-branch names with the local pre-commit configuration. `mise run setup` installs the hook; it can also be installed again explicitly with:

```sh
mise run hooks
```

Branches must use one of the supported types followed by `/` and a lowercase
branch slug:

```text
feat/add-adapter
feat/sdk-add-adapter
docs/contributing-guide
chore/update-deps
```

Supported types are `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`,
`build`, `ci`, `chore`, and `revert`. Scopes are intentionally open, so names
such as `sdk`, `cli`, `website`, or `deps` do not need to be registered first.
The shared branches `main`, `master`, and `develop` are also allowed.

## Commit messages

Commit subjects must follow [Conventional Commits](https://www.conventionalcommits.org/) using the same type list:

```text
<type>(<scope>)!: <subject>
```

The scope is optional and lowercase, `!` marks a breaking change, and the subject line is at most 72 characters:

```text
feat(cli): add tuff diff command
fix: fail release when checksums are missing
docs(readme): streamline quick start
chore!: drop lockfile schema v1
```

The `commit-msg` hook (installed by `mise run hooks`) enforces this locally. The `Commit Message` workflow re-checks every commit in a pull request and the pull request title, since the title becomes the subject on squash merge.

## Changelog and releases

User-facing changes should update the `Unreleased` section of [CHANGELOG.md](CHANGELOG.md) under `Added`, `Improved`, or `Fixed`. Release preparation moves those entries into a dated version section and updates the comparison links without rewriting an existing release tag.

The website renders `CHANGELOG.md` as its [Changelog page](https://tuffcli.dev/changelog/). The page is generated at build time by `website/scripts/sync-changelog.mjs`, so there is nothing to update by hand; keep the root file as the only copy.

The hooks specification under `spec/hooks/` is generated too: `hooks-spec.json` is what `tuff hooks spec --json` prints and the tables in `SPEC.md` are rendered from it. After changing `crates/tuff-hooks-spec` or an adapter's compatibility matrix, run `mise run spec-sync` and commit the result; `mise run check` and a pre-commit hook fail on drift. The prose around the tables is edited by hand, and a change to the vocabulary itself moves the specification's own version, as its section 9 describes.

To cut a release:

1. Open a `chore: prepare tuffcli X.Y.Z release` pull request that moves the `Unreleased` entries into a dated `[X.Y.Z]` section, adds the section's comparison link, bumps `version` and the internal path-dependency pins in the root `Cargo.toml`, refreshes `Cargo.lock`, and updates the `--version` integration test. Every pull request merged since the previous tag must have an entry; write one from the pull request description if it was skipped.
2. Merge it, then tag the merge commit with `git tag vX.Y.Z && git push origin vX.Y.Z`. The tag triggers the release, crates.io, and PyPI workflows and pushes the Homebrew formula.
3. Confirm the GitHub release, crates.io, PyPI, and the Homebrew tap all show the new version.

The `tuff-console` crate is published by the `tuff-console` job of `crates.yml`, after `tuff-core` and before `tuffcli`. Before the first tag that includes it, a maintainer reserves the crate name on crates.io and adds its trusted publisher (owner `kannandreams`, repository `tuff`, workflow `crates.yml`, environment `release-tuff-console`). A trusted publisher can only be added to a crate that exists, and the crate needs this release's `tuff-core` on crates.io to compile, so the reservation is done after that crate is published or with a stub.

GitHub release notes are generated automatically from merged pull requests. Apply the `enhancement` or `feature`, `documentation`, `dependencies`, `maintenance`, `ci`, `bug`, or `fix` label when one category clearly applies; unlabeled changes remain visible under “Other Changes.” The curated changelog remains the authoritative summary of user-visible behavior.

## Checks

Before opening a pull request, run the checks relevant to your change. The canonical full validation is:

```sh
mise run check
```

Use direct commands when a narrower check is sufficient:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
npm --prefix website audit --audit-level=low
npm --prefix website run check
npm --prefix website run build
```

Build the docs site whenever documentation, Astro configuration, styles, or
landing-page assets change. Run `npm --prefix website run check` when changing
Astro or TypeScript code.

## CLI and compatibility changes

Tuff's CLI output and file formats are user-facing interfaces. Contributions
should:

- Keep behavior deterministic and friendly to CI.
- Prefer clear, actionable errors over hidden fallback behavior.
- Add or update tests for user-visible command behavior.
- Preserve compatibility with existing `tuff.lock` files unless a migration
  is intentional, tested, and documented.
- Keep adapter-specific behavior inside the relevant adapter crate.
- Update the README or documentation when commands, formats, or workflows
  change.
- Avoid committing generated files, coverage reports, build outputs, local
  caches, or machine-specific configuration.

For changes that affect installation, run the local smoke test as well:

```sh
mise run smoke-install
```

## MCP catalog entries

The built-in catalog in `crates/tuff-core/assets/mcp-catalog.toml` is what `tuff add mcp <id>` installs by name, and `tuff mcp catalog`, the website, and the VS Code extension list it. A server outside the catalog can still be installed from a local folder, a Git URL, or the MCP registry. An entry is accepted when it meets all of these:

- **It is the official server.** The maintainers of the service or tool publish it, or it is a reference server from the `modelcontextprotocol/servers` repository. Community forks of a service's API are not added.
- **The invocation comes from a primary source.** The command, arguments, transport, URL, and headers are shown in the server's own README or documentation, and the pull request links to that page. Nothing is inferred from a `package.json` or a search result.
- **It is maintained.** The server is not archived or deprecated, and it had a release or commit in the last six months.
- **It is useful to someone working with a coding agent.** Developer tools, documentation, issue trackers, data sources, and products whose users run agents are in scope.
- **Secrets are environment variable names.** `env` and `headers` name the variables the developer exports, and the description says where the value comes from.
- **The description states what the server needs and where data goes.** Required running services, accounts, or keys are named, and so is any feature that sends data to a third party.

A maintainer of the server is welcome to open the pull request. Say so in the description. The licence of the server does not decide inclusion, but a server that is not open source has its licence noted in a comment above the entry.

A new entry starts at `version = "1.0.0"`, and a change to an existing entry bumps only that entry's version. Add the id to the catalog list in `crates/tuff-cli/assets/tuff-cli-guide.md` and run `mise run plugin-sync`, add a line under `Unreleased` in `CHANGELOG.md`, and run `cargo test -p tuff-core catalog` and `cargo test -p tuffcli mcp_catalog`.

## Pull requests

Keep each pull request focused on one problem or feature. A useful pull
request includes:

- A short summary of what changed and why.
- A link to the related issue, when one exists.
- Tests or documentation updates that support the change.
- An example of changed CLI output or workflow when behavior is user-visible.
- The exact checks you ran.

Avoid unrelated refactoring, formatting-only churn, or opportunistic changes
in the same pull request. Maintainers may ask for a pull request to be split,
re-scoped, or updated before review.

## Questions and ideas

If you are unsure whether something belongs in an issue or pull request, start
with an issue. Real-world examples, questions, and feedback are useful even
when they do not come with code.

Please also follow the project's [Code of Conduct](CODE_OF_CONDUCT.md).
