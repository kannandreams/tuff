# Coral Tool Examples

These examples show different shapes of tool capabilities that Coral can
install, track, diff, update, and remove.

Install one into the configured default agent:

```sh
coral add examples/tools/python-script-tool
```

Then inspect the emitted tool files:

```sh
coral list --type tool
```

Coral validates and copies these tools, but it does not execute them during
install. Runtime dependencies are declared in each `coral.toml` and remain the
responsibility of the agent runtime or developer environment.

Only tools with `mcp = true` in `[implementation]` are registered in the
agent's MCP config.
