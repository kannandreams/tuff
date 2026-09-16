---
title: Clean Up
description: Delete generated files, stop tracking capabilities, migrate the lockfile, and clear the cache.
---

## `tuff delete`

Delete Tuff-generated capability files for explicitly selected agents:

```sh frame="terminal"
# Delete generated files for one agent
tuff delete <id> -a open-agents

# Delete generated files for multiple agents
tuff delete <id> -a open-agents -a claude

# Delete from global scope
tuff delete <id> -a open-agents --scope global

# Delete files with local modifications
tuff delete <id> -a open-agents --force
```

When `-a/--harness` is omitted, `delete` uses the configured agent. It removes emitted files, their baselines,
and generated tool MCP entries. It never deletes the original capability source
directory. Modified generated files require `--force`. In-place added capabilities
cannot be deleted; use `tuff untrack` instead.

## `tuff untrack`

Stop tracking a capability for explicitly selected agents while preserving its
agent files and manifest:

```sh frame="terminal"
# Stop tracking an in-place added skill for the default agent
tuff untrack my-skill

# Stop tracking several agents
tuff untrack my-skill -a open-agents -a claude

# Stop tracking a global capability
tuff untrack my-skill -a open-agents --scope global
```

`untrack` removes the selected lockfile entry and baseline. It preserves the
capability files, source directories, and MCP configuration.
The lockfile itself remains in place, even when it contains no capabilities.

## `tuff lock migrate`

Rewrite `tuff.lock` in the current schema version, changing nothing else. Tuff reads older versions transparently, so this is only needed to land the migration as its own commit; on a current file it is a no-op. See [Migrating from an older schema](/concepts/lockfile#migrating-from-an-older-schema).

```sh frame="terminal"
tuff lock migrate
```

## `tuff cache clear`

Delete Tuff's disposable machine-local cache of materialized trees and source
clones. This does not remove project capability files or lockfile entries:

```sh frame="terminal"
tuff cache clear
```
