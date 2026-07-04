---
title: Harness Adapters
description: How Coral maps one managed primitive model into target-specific output.
---

Different coding harnesses expect different file layouts, config surfaces, and conventions.
Coral's adapter layer is meant to let teams manage one source model and then emit harness-specific output.

The MVP starts with the Codex skill target. Additional harnesses should be added through explicit adapter
behavior rather than ad hoc copies or per-project special casing.
