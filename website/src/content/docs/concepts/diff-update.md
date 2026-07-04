---
title: Diffing & Updates
description: How Coral compares installed state and why updates matter.
---

Coral already proves the first diffing loop by comparing installed content against a recorded baseline.

The longer-term direction is broader:

- diff local installed state against baseline
- detect upstream source changes
- compare baseline, local, and upstream state together
- preserve intentional local changes during updates

That future merge/update behavior is one of Coral's most important product bets.
