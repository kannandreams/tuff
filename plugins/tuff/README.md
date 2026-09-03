# Tuff plugin for Claude Code

Makes [Tuff](https://tuffcli.dev) drivable from a Claude Code session. Ask the agent to install a skill, wire an MCP server, or check for drift, and it uses the Tuff CLI instead of copying files by hand.

## Requirements

The plugin calls the `tuff` binary. Install it first:

```sh
curl -fsSL https://raw.githubusercontent.com/kannandreams/tuff/main/install.sh | sh
```

Homebrew, Cargo, and pip installs are documented at <https://tuffcli.dev/installation/>. Verify with `tuff --version`.

## Install

```sh
claude plugin marketplace add kannandreams/tuff
claude plugin install tuff@tuff
```

From inside a session, use `/plugin marketplace add kannandreams/tuff` and then `/plugin install tuff@tuff`.

## What it ships

One skill, `tuff-cli-guide`, which is the same agent-facing reference `tuff init` installs into a project. It covers install, packs, inspection, update and merge, CI validation, the directory model, and drift status values.

The skill file is generated from `crates/tuff-cli/assets/tuff-cli-guide.md` in this repository. Edit that asset, then run `mise run plugin-sync`; `mise run plugin-check` fails if the two drift apart.

The marketplace entry tracks this directory on `main`, so a merged change reaches users as soon as they refresh the marketplace. Bump `version` in `plugin.json` when the skill changes, since that is what tells an installed copy an update exists.

## License

MIT, the same as the rest of the repository.
