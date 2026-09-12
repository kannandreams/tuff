// Renders the published hooks specification as a documentation page and
// serves its machine-readable files.
//
// The specification has one home, `spec/hooks/` in the repository, where
// `scripts/check-spec.sh` keeps it generated from the crates. The website
// never carries a second copy that can drift: this script runs before every
// `astro check`, `astro build`, and `astro dev` (see the pre-scripts in
// package.json) and writes src/content/docs/spec/hooks.md from SPEC.md, plus
// public/spec/hooks/hooks-spec.json and hooks-spec.schema.json so the URLs
// the specification names resolve on tuffcli.dev. All three are gitignored.

import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const specDir = resolve(here, "../../spec/hooks");
const page = resolve(here, "../src/content/docs/spec/hooks.md");
const publicDir = resolve(here, "../public/spec/hooks");

const prose = readFileSync(resolve(specDir, "SPEC.md"), "utf8");
const lines = prose.split("\n");
if (!lines[0].startsWith("# ")) {
  throw new Error("spec/hooks/SPEC.md must start with a top-level heading");
}
const version = /^Version (\d+\.\d+\.\d+)\./m.exec(prose)?.[1];
if (!version) {
  throw new Error("spec/hooks/SPEC.md must declare its version");
}
const document = JSON.parse(readFileSync(resolve(specDir, "hooks-spec.json"), "utf8"));
if (document.spec_version !== version) {
  throw new Error(
    `spec/hooks/SPEC.md declares ${version} but hooks-spec.json is ${document.spec_version}; run 'mise run spec-sync'`,
  );
}

// Starlight renders the title from the frontmatter and the page's own
// headings start one level down, so the H1 goes and H2/H3 stay as they are.
const body = lines.slice(1).join("\n").trimStart();

mkdirSync(dirname(page), { recursive: true });
writeFileSync(
  page,
  `---
title: Hooks Specification
description: The hook vocabulary Tuff implements, how each harness renders it, and what a second implementation must do to be compatible. Version ${version}.
---

<!-- Generated from spec/hooks/SPEC.md by website/scripts/sync-hooks-spec.mjs. Edit the source, not this file. -->

${body}`,
);

mkdirSync(publicDir, { recursive: true });
for (const name of ["hooks-spec.json", "hooks-spec.schema.json"]) {
  copyFileSync(resolve(specDir, name), resolve(publicDir, name));
}
