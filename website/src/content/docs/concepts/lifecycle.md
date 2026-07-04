---
title: Lifecycle & Drift Detection
description: How Coral records baselines and reports local changes.
---

Coral is built around lifecycle management, not just installation.

The core loop is:

1. install a primitive into a project target
2. record the install-time baseline
3. allow the project to customize the installed artifact
4. detect drift relative to the recorded baseline
5. make that drift visible through listing, diffing, and later update/merge workflows

This is the main reason Coral exists. Teams need project-owned primitives that can evolve without
losing provenance.
