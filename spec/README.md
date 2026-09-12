# Tuff specifications

Published, versioned descriptions of the formats Tuff implements, written so that something other than Tuff can implement them too.

| Specification | Version | Source of truth |
|---|---|---|
| [Hooks](hooks/SPEC.md) | 0.1.0 | `crates/tuff-hooks-spec` and the adapter matrices, via `tuff hooks spec --json` |

Every specification here is descriptive of running code, never ahead of it. The tables and JSON documents are generated from the crates by `mise run spec-sync`, and `mise run check` fails when they drift. A specification grows only where a second implementer shows up to need it.
