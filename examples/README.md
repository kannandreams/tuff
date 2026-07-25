# Tuff examples

These directories contain runnable capability examples for Tuff command
documentation and contributor smoke tests. Organize examples by capability
type so the install path matches the capability being demonstrated:

```text
examples/
  skills/<id>/
  tools/<id>/
  hooks/<id>/
  workflows/<id>/
```

For test-only inputs such as malformed manifests or legacy lockfiles, use
`tests/fixtures/` instead.
