---
title: Scopes & Overrides
description: How project and global capability state interact today.
---

Tuff currently has two scopes:

- `project`
- `global`

The important rule is simple: project scope wins.

## Scope Types

### Project scope

Project-scoped capabilities live in the repository and are tracked by:

- `tuff.lock`
- `tuff.config.json` (optional project preferences)

This is the default scope for `tuff add`, `tuff list`, `tuff diff`, and
`tuff update`.

### Global scope

Global scope is for personal capabilities shared across projects. It uses the platform's
XDG-style Tuff config, state, and cache directories:

- `tuff.lock` in Tuff's user state directory
- verified trees in Tuff's user cache directory
- `~/.agents/` or other global emitted files

Use it explicitly:

```sh frame="terminal"
tuff init --global
tuff add ./my-skill --global
tuff update my-skill --scope global
```

## Resolution order

When Tuff looks up an installed capability by id, it resolves in this order:

1. project scope
2. global scope

That means a project copy shadows a global copy with the same id.

## Override Behavior

If both scopes contain the same capability id:

- the project copy is the active one for that repo
- the global copy remains installed, but is shadowed there

Tuff surfaces this in status output:

- project entries can show: `[overrides global: won't receive global updates]`
- global entries can show: `[shadowed by project copy]`

## Example: project copy overrides global

```sh frame="terminal"
# Install globally
tuff add ./company-review --global

# In a repo, install a project-specific copy with the same id
tuff add ./company-review-custom

tuff status
```

In that repository, Tuff resolves the project copy first.

## Collision warnings

If a capability id already exists globally and you install a project copy from a different source,
Tuff warns that the project copy will take precedence.

This is especially useful when:

- a company skill is installed globally
- a repository wants to pin or fork its own version

## Scope-aware commands

### Commands that default to project scope

```sh frame="terminal"
tuff add ./my-skill
tuff list
tuff diff my-skill
tuff update my-skill
```

### Commands that can target global scope

```sh frame="terminal"
tuff add ./my-skill --global
tuff list --scope global
tuff delete my-skill --scope global
tuff untrack my-skill --scope global
tuff update my-skill --scope global
```

## Current Limits

Tuff does not currently support a deeper layered model such as:

- company
- team
- project
- personal local

Today the scope model is intentionally small: one repo-local layer plus one global layer, with
clear precedence and explicit warnings when one shadows the other.
