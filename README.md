<p align="center"><img src="assets/tuff-readme-banner.png" alt="Tuff banner" width="1100" /></p>

<h1 align="center">Tuff</h1>

<p align="center"><strong>Make your coding-agent playbook reproducible.</strong></p>

<p align="center">Install, version, diff, and update the skills, tools, hooks, and workflows that make your agents useful.</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT" /></a>
  <a href="https://crates.io/crates/tuffcli"><img src="https://img.shields.io/crates/v/tuffcli.svg" alt="crates.io" /></a>
  <a href="https://pypi.org/project/tuffcli/"><img src="https://img.shields.io/pypi/v/tuffcli.svg" alt="PyPI" /></a>
</p>

Agent capabilities quickly become part of your engineering infrastructure. But once skills and automation are copied across `.agents/`, `.claude/`, and `.cursor/`, teams lose track of where they came from, what changed, and whether every developer is running the same version.

**Tuff turns that copy-paste into a managed lifecycle.** Capabilities stay as ordinary, project-owned files while Tuff records their source and baseline, emits harness-native output, exposes local drift, and makes upstream updates reviewable.

## See it work in 60 seconds

Install Tuff on macOS or Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/kannandreams/tuff/main/install.sh | sh
```

Then run this inside a project:

```sh
# Initialize project state.
tuff init

# Install a real Rust skill directly from a public Git repository.
tuff add skill https://github.com/pproenca/dot-skills rust-implement \
  --agent open-agents

# Create your own tracked skill for two harnesses.
tuff create skill release-checklist \
  --agent open-agents \
  --agent claude

# Edit .agents/skills/release-checklist/SKILL.md, then review the change.
tuff list
tuff diff release-checklist
tuff update release-checklist
tuff check
```

Tuff installs [`rust-implement`](https://github.com/pproenca/dot-skills/tree/master/skills/.curated/rust-implement) into `.agents/skills/`, records its Git revision, and creates your project skill in `.agents/` and `.claude/`. Each target is tracked in `tuff.lock` with a pristine baseline for drift checks. Commit the capability files and lockfile so the whole team gets the same setup.

The `open-agents` target works with Codex, Cursor, OpenCode, GitHub Copilot, Gemini CLI, Roo, Cline, and Windsurf. Tuff also ships dedicated adapters for Claude Code, Codex, and Cursor.

## What changes with Tuff

| Without Tuff | With Tuff |
|---|---|
| Agent files are copied between repos and machines. | Capabilities are created, adopted, or installed with a repeatable command. |
| Local edits become invisible forks. | `tuff list` and `tuff diff` show drift from the recorded baseline. |
| Every harness needs hand-maintained configuration. | Adapters emit the capability into each harness's native layout. |
| Pulling an upstream update risks losing local changes. | `tuff diff --upstream` previews the change, and updates refuse to overwrite local drift unless you explicitly force them. |
| CI cannot tell whether agent setup has changed. | `tuff check` validates tracked capabilities and fails on drift or missing files. |

## One lifecycle for every capability

| Capability | What it gives an agent |
|---|---|
| **Skill** | Reusable instructions, conventions, and domain context. |
| **Tool** | Executable behavior with a clear contract. |
| **Hook** | Automation triggered at meaningful agent events. |
| **Workflow** | A composable sequence of capabilities. |

All four use the same lifecycle:

```text
local files / git repository / existing agent assets
                         │
                    tuff create
                      or tuff add
                         │
             ┌───────────┼───────────┐
             ▼           ▼           ▼
         .agents/    .claude/    .cursor/
             └───────────┬───────────┘
                         ▼
             tracking metadata + baseline
                         │
              list → diff → update → check
```

Tuff manages the lifecycle around these files; Git remains the source of truth for your repository, and your existing agent runtime continues to execute the capabilities.

## Common workflows

```sh
# Adopt an existing project skill without moving it.
tuff add skill .agents/skills/security-review --agent open-agents

# Install a capability from Git into multiple harnesses.
tuff add skill https://github.com/owner/agent-capabilities security-review \
  --agent open-agents \
  --agent claude

# Review and reconcile an upstream change.
tuff outdated
tuff diff security-review --upstream
tuff update security-review --check
tuff update security-review
```

Tuff supports project and global scopes. Project capabilities are designed to be committed with the repository; global capabilities are useful for personal capabilities shared across projects.

## Other installation options

```sh
cargo install tuffcli       # crates.io
uv tool install tuffcli     # PyPI, isolated environment
pip install tuffcli         # PyPI

brew tap kannandreams/tuff  # Homebrew
brew install tuff
```

The package is named `tuffcli`; every installation method provides the `tuff` command.

## Documentation

- [Getting started](https://tuffcli.dev/getting-started/)
- [When to use Tuff](https://tuffcli.dev/usage-scenarios/)
- [CLI reference](https://tuffcli.dev/cli/)
- [Lifecycle and drift detection](https://tuffcli.dev/concepts/lifecycle/)

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for project guidelines and [AGENTS.md](AGENTS.md) for repository layout, development commands, and verification guidance. Please follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## License

Tuff is released under the [MIT License](LICENSE).
