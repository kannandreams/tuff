---
title: Lockfile Reference
description: What Coral records in project state.
---

Coral records project state in `.coral/lock.json`.

The current lockfile tracks, per installed primitive:

- primitive id
- version
- source path
- installed target path
- baseline content hash
- installed content hash

That lockfile is the foundation for lifecycle operations such as drift detection, diffs, provenance,
and future update/merge behavior.
