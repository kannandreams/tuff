// Renders the repository's CHANGELOG.md as a documentation page.
//
// The changelog has one home, the repository root, so the website never
// carries a second copy that can drift. This script runs before every
// `astro check`, `astro build`, and `astro dev` (see the pre-scripts in
// package.json) and writes src/content/docs/changelog.md, which is
// gitignored. It strips the top-level heading, which Starlight renders
// from the frontmatter title instead.

import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const source = resolve(here, "../../CHANGELOG.md");
const target = resolve(here, "../src/content/docs/changelog.md");

const changelog = readFileSync(source, "utf8");
const lines = changelog.split("\n");
if (!lines[0].startsWith("# ")) {
  throw new Error(`${source} must start with a top-level heading`);
}

const body = lines
  .slice(1)
  .join("\n")
  .replace(/\bin this file\b/, "on this page")
  .trimStart();

const page = `---
title: Changelog
description: Every user-facing change to Tuff, by release.
---

<!-- Generated from CHANGELOG.md by website/scripts/sync-changelog.mjs. Edit the source, not this file. -->

${body}`;

mkdirSync(dirname(target), { recursive: true });
writeFileSync(target, page);
