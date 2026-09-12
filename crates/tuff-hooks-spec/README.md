# tuff-hooks-spec

`tuff-hooks-spec` defines Tuff's canonical hook vocabulary and compatibility
matrix types.

The crate is intentionally small and Tuff-owned. It exists so Tuff-standard
hooks can be described once and rendered into native harness hook formats where
an adapter declares compatible support. Native harness hook fragments remain
outside this layer and are passed through by Tuff adapters.

Harness version bounds are optional. When Tuff knows that behavior changed in a
specific harness version, adapters can fill in `since_harness_version` and
`until_harness_version`. When the version is unknown, those fields should remain
empty rather than using placeholder values.

The vocabulary this crate defines is published as the [Tuff Hooks
Specification](https://tuffcli.dev/spec/hooks/), together with every
adapter's compatibility matrix, a conformance checklist, and a JSON Schema for
the machine-readable form. `tuff hooks spec --json` prints that document from
the running binary, and the repository's `spec/hooks/` copy is generated from
it, so the published specification cannot say something the code does not do.
