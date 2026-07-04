---
title: Scopes & Overrides
description: How Coral should eventually reason about global, project, and local differences.
---

Teams eventually need more than one place to define primitives. A company may maintain shared packs,
a product repo may install and customize them, and an engineer may still want local preferences.

Scopes and overrides define how those layers interact without losing clarity about which source of truth
is currently active.

This is roadmap work, but it should become a first-class concept before Coral grows into broader
multi-primitive update behavior.
