// Renders the published pack JSON specification as a documentation page and
// serves its machine-readable files, the same way sync-hooks-spec.mjs does
// for the hooks specification.
//
// The specification lives in `spec/packs/` in the repository, where
// `scripts/check-pack-spec.sh` keeps its example generated from the code.
// This script runs before every `astro check`, `astro build`, and
// `astro dev` and writes src/content/docs/spec/packs.md from SPEC.md, plus
// public/spec/packs/inspect.schema.json and inspect.example.json. All three
// are gitignored.

import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const specDir = resolve(here, "../../spec/packs");
const page = resolve(here, "../src/content/docs/spec/packs.md");
const publicDir = resolve(here, "../public/spec/packs");

const prose = readFileSync(resolve(specDir, "SPEC.md"), "utf8");
const lines = prose.split("\n");
if (!lines[0].startsWith("# ")) {
  throw new Error("spec/packs/SPEC.md must start with a top-level heading");
}
const version = /^Version (\d+)\./m.exec(prose)?.[1];
if (!version) {
  throw new Error("spec/packs/SPEC.md must declare its version");
}
const example = JSON.parse(readFileSync(resolve(specDir, "inspect.example.json"), "utf8"));
if (String(example.schema) !== version) {
  throw new Error(
    `spec/packs/SPEC.md declares version ${version} but inspect.example.json is schema ${example.schema}; run 'mise run spec-sync'`,
  );
}

const body = lines.slice(1).join("\n").trimStart();

mkdirSync(dirname(page), { recursive: true });
writeFileSync(
  page,
  `---
title: Pack JSON Specification
description: The JSON documents Tuff prints about packs, for applications that install packs for their users, and the host tables Tuff carries for them. Version ${version}.
---

<!-- Generated from spec/packs/SPEC.md by website/scripts/sync-pack-spec.mjs. Edit the source, not this file. -->

${body}`,
);

mkdirSync(publicDir, { recursive: true });
for (const name of ["inspect.schema.json", "inspect.example.json"]) {
  copyFileSync(resolve(specDir, name), resolve(publicDir, name));
}
