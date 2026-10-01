#!/usr/bin/env node
// Builds the in-browser Tuff Console preview the website embeds on its
// landing page: the console's own UI files, a snapshot of every API response
// the UI asks for from `tuff console serve --demo`, and preview.js, which
// answers the UI's /api/v1 requests from that snapshot.
//
//   cargo build -p tuffcli
//   node scripts/build-console-preview.mjs
//
// Options:
//   --bin <path>   the tuff binary (default target/debug/tuff)
//   --out <dir>    where to write (default website/public/console-preview)
//   --check        build into a temporary folder and fail if the committed
//                  preview differs, apart from timestamps (`mise run check`
//                  and `mise run docs-deploy` run this)
//
// Run it again whenever crates/tuff-console/ui or the demo data changes.

import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, writeFileSync, rmSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const at = args.indexOf(name);
  return at >= 0 ? args[at + 1] : fallback;
};
const bin = resolve(option("--bin", "target/debug/tuff"));
const committed = resolve(option("--out", "website/public/console-preview"));
const check = args.includes("--check");
const out = check ? mkdtempSync(join(tmpdir(), "console-preview-")) : committed;
const ui = resolve("crates/tuff-console/ui");

function startConsole() {
  return new Promise((done, fail) => {
    const child = spawn(bin, ["console", "serve", "--demo", "--addr", "127.0.0.1:0"], { stdio: ["ignore", "pipe", "inherit"] });
    let buffer = "";
    child.stdout.on("data", (chunk) => {
      buffer += chunk;
      const match = buffer.match(/Console listening on (http:\/\/\S+)/);
      if (match) done({ url: match[1], stop: () => child.kill() });
    });
    child.on("error", fail);
    child.on("exit", (code) => fail(new Error(`tuff exited early with ${code}`)));
  });
}

const server = await startConsole();
const responses = {};
async function grab(path) {
  const response = await fetch(`${server.url}/api/v1${path}`);
  if (!response.ok) throw new Error(`${path}: HTTP ${response.status}`);
  const body = await response.json();
  responses[path] = body;
  return body;
}

try {
  const settings = await grab("/settings");
  // The preview has no server of its own to describe.
  settings.server = { preview: true, demo: true, version: settings.server.version };
  const { projects } = await grab("/projects");
  for (const project of projects) {
    await grab(`/projects/${project.id}`);
    await grab(`/projects/${project.id}/reports`);
  }
  const capabilities = await grab("/capabilities");
  const list = capabilities.capabilities || [];
  for (const type of new Set(list.map((c) => c.type))) {
    await grab(`/capabilities?type=${encodeURIComponent(type)}`);
  }
  for (const c of list) {
    await grab(`/capabilities/${encodeURIComponent(c.type)}/${encodeURIComponent(c.id)}`);
  }
  await grab("/harnesses");
  await grab("/policies");
  const all = await grab("/events?limit=1000");
  delete responses["/events?limit=1000"];

  const snapshot = {
    takenAt: new Date().toISOString(),
    responses,
    events: all.events,
    kinds: all.kinds,
  };

  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  writeFileSync(join(out, "data.json"), JSON.stringify(snapshot));
  writeFileSync(join(out, "app.css"), readFileSync(join(ui, "app.css")));
  writeFileSync(join(out, "app.js"), readFileSync(join(ui, "app.js")));
  writeFileSync(join(out, "preview.js"), readFileSync(resolve("scripts/console-preview.js")));
  const index = readFileSync(join(ui, "index.html"), "utf8")
    .replace('href="/assets/app.css"', 'href="app.css"')
    .replace('<script src="/assets/app.js"></script>', '<script src="preview.js"></script>\n<script src="app.js"></script>')
    .replace("<title>Tuff Console</title>", "<title>Tuff Console preview</title>");
  if (!index.includes('src="preview.js"')) throw new Error("index.html no longer loads /assets/app.js the way this script expects");
  writeFileSync(join(out, "index.html"), index);
  if (check) {
    const stale = compare(out, committed);
    rmSync(out, { recursive: true, force: true });
    if (stale.length) {
      console.error(`The landing page's console preview is out of date: ${stale.join(", ")}.`);
      console.error("Run `mise run console-preview` and commit website/public/console-preview.");
      process.exitCode = 1;
    } else {
      console.log("The console preview matches the console UI and demo data.");
    }
  } else {
    console.log(`wrote the console preview to ${out} (${Object.keys(responses).length} responses, ${all.events.length} events)`);
  }
} finally {
  server.stop();
}

/* Files that differ between a fresh build and the committed preview.
   data.json is compared without its timestamps, which follow the clock. */
function compare(fresh, saved) {
  const ISO = /"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?Z"/g;
  const normal = (name, text) => (name === "data.json" ? text.replace(ISO, '"<time>"') : text);
  const stale = [];
  for (const name of readdirSync(fresh)) {
    const path = join(saved, name);
    if (!existsSync(path)) { stale.push(`${name} is missing`); continue; }
    if (normal(name, readFileSync(join(fresh, name), "utf8")) !== normal(name, readFileSync(path, "utf8"))) stale.push(name);
  }
  return stale;
}
