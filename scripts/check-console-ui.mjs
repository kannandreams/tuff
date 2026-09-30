#!/usr/bin/env node
// Loads every Tuff Console view in a headless Chromium, in light and dark,
// and fails on console errors, page errors, failed or non-2xx requests, and
// any request that leaves the console's own origin.
//
//   cargo build -p tuffcli
//   PLAYWRIGHT_DIR=/path/to/node_modules/playwright node scripts/check-console-ui.mjs --out shots/
//
// Playwright is not a dependency of this repository. Point PLAYWRIGHT_DIR at
// an installed copy, or install it next to the script (`npm i playwright`)
// and omit the variable. Options:
//   --bin <path>   the tuff binary (default target/debug/tuff)
//   --out <dir>    write a screenshot of every view here
//   --url <url>    check a console that is already running, in place of starting
//                  one with --demo (the empty-console check is skipped)

import { spawn } from "node:child_process";
import { mkdirSync } from "node:fs";
import { resolve, join } from "node:path";
import { pathToFileURL } from "node:url";

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const at = args.indexOf(name);
  return at >= 0 ? args[at + 1] : fallback;
};
const bin = resolve(option("--bin", "target/debug/tuff"));
const outDir = option("--out", "");
const externalUrl = option("--url", "");

async function loadPlaywright() {
  const dir = process.env.PLAYWRIGHT_DIR;
  if (dir) return import(pathToFileURL(resolve(dir, "index.mjs")).href);
  return import("playwright");
}
const playwright = await loadPlaywright();
const chromium = playwright.chromium || playwright.default.chromium;

function startConsole(extra) {
  return new Promise((done, fail) => {
    const child = spawn(bin, ["console", "serve", "--addr", "127.0.0.1:0", ...extra], { stdio: ["ignore", "pipe", "inherit"] });
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

const failures = [];
const fail = (where, message) => failures.push(`${where}: ${message}`);

async function checkPage(browser, { url, scheme, viewport, route, label, settle }) {
  const context = await browser.newContext({ colorScheme: scheme, viewport });
  const page = await context.newPage();
  const where = `${label} [${scheme}, ${viewport.width}px]`;
  const origin = new URL(url).origin;
  page.on("console", (message) => { if (message.type() === "error") fail(where, `console error: ${message.text()}`); });
  page.on("pageerror", (error) => fail(where, `page error: ${error.message}`));
  page.on("requestfailed", (request) => fail(where, `request failed: ${request.url()} ${request.failure()?.errorText}`));
  page.on("response", (response) => { if (response.status() >= 400) fail(where, `HTTP ${response.status()} for ${response.url()}`); });
  page.on("request", (request) => {
    const target = request.url();
    if (!target.startsWith("data:") && new URL(target).origin !== origin) fail(where, `request left the console origin: ${target}`);
  });
  await page.goto(`${url}/${route}`, { waitUntil: "networkidle" });
  await page.waitForSelector("#main .view", { timeout: 10000 }).catch(() => fail(where, "the view never rendered"));
  if (settle) await settle(page, where);
  if ((await page.locator("#main .error").count()) > 0) fail(where, `the view shows an error: ${await page.locator("#main .error").first().innerText()}`);
  const heading = (await page.locator("#main h1, #main h2").first().innerText().catch(() => "")).trim();
  if (!heading) fail(where, "the view has no heading");
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth + 1);
  if (overflow) fail(where, "the page scrolls sideways");
  if (outDir) {
    mkdirSync(outDir, { recursive: true });
    await page.screenshot({ path: join(outDir, `${scheme}-${viewport.width}-${label}.png`), fullPage: true });
  }
  await context.close();
}

const browser = await chromium.launch();
const servers = [];
try {
  const demo = externalUrl ? { url: externalUrl.replace(/\/$/, ""), stop() {} } : await startConsole(["--demo"]);
  servers.push(demo);
  const projects = await (await fetch(`${demo.url}/api/v1/projects`)).json();
  if (!projects.projects.length) throw new Error("the console has no projects to render");
  const byName = Object.fromEntries(projects.projects.map((p) => [p.name, p.id]));

  const routes = [
    ["dashboard", "#/"],
    ["projects", "#/projects"],
    ...projects.projects.map((p) => [`project-${p.name}`, `#/projects/${p.id}`]),
    ["capabilities", "#/capabilities"],
    ["capabilities-skill", "#/capabilities?type=skill"],
    ["capability-code-review", "#/capabilities/skill/code-review"],
    ["harnesses", "#/harnesses"],
    ["policies", "#/policies"],
    ["audit", "#/audit"],
    ["audit-drift", "#/audit?kind=drift_detected"],
    ["settings", "#/settings"],
  ];
  for (const scheme of ["light", "dark"]) {
    for (const [label, route] of routes) {
      await checkPage(browser, { url: demo.url, scheme, viewport: { width: 1280, height: 900 }, route, label });
    }
  }
  // A phone-sized pass over the views with the widest tables.
  for (const [label, route] of [["dashboard", "#/"], ["projects", "#/projects"], ["project-payments-api", `#/projects/${byName["payments-api"]}`], ["harnesses", "#/harnesses"]]) {
    await checkPage(browser, { url: demo.url, scheme: "light", viewport: { width: 390, height: 844 }, route, label });
  }

  // The Sample data chip shows in demo mode, and rows and the theme button work.
  {
    const context = await browser.newContext({ colorScheme: "light", viewport: { width: 1280, height: 900 } });
    const page = await context.newPage();
    page.on("pageerror", (error) => fail("interaction", `page error: ${error.message}`));
    await page.goto(`${demo.url}/#/projects`, { waitUntil: "networkidle" });
    await page.waitForSelector("#main tr[data-href]");
    if (!(await page.locator("#demo-chip").isVisible())) fail("interaction", "the Sample data chip is hidden in demo mode");
    const before = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    await page.click("#theme-btn");
    const after = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    if (before === after) fail("interaction", "the theme button did not change the page");
    await page.locator("#main tr[data-href]").first().click();
    await page.waitForFunction(() => /#\/projects\/\d+$/.test(location.hash));
    await page.waitForSelector("#main .crumb");
    await page.goBack();
    await page.waitForSelector("#main tr[data-href]");
    await page.goto(`${demo.url}/#/audit`, { waitUntil: "networkidle" });
    await page.selectOption('[data-filter="kind"]', "drift_detected");
    await page.waitForFunction(() => location.hash.includes("kind=drift_detected"));
    await page.waitForFunction(() => {
      const kinds = [...document.querySelectorAll("#main .ev [data-kind]")].map((n) => n.dataset.kind);
      return kinds.length > 0 && kinds.every((kind) => kind === "drift_detected");
    }, null, { timeout: 5000 }).catch(() => fail("interaction", "the audit filter did not narrow the list to drift_detected"));
    await context.close();
  }

  // A console with no reports explains how to publish.
  if (!externalUrl) {
    const empty = await startConsole([]);
    servers.push(empty);
    for (const scheme of ["light", "dark"]) {
      await checkPage(browser, {
        url: empty.url, scheme, viewport: { width: 1280, height: 900 }, route: "#/", label: "dashboard-empty",
        settle: async (page, where) => {
          const text = await page.locator("#main").innerText();
          if (!text.includes("tuff console publish") || !text.includes("No reports yet")) fail(where, "the empty state does not explain publishing");
          if ((await page.locator('#main a[href="https://tuffcli.dev/cli/console/"]').count()) !== 1) fail(where, "the empty state does not link the documentation");
          if (await page.locator("#demo-chip").isVisible()) fail(where, "the Sample data chip shows outside demo mode");
        },
      });
      await checkPage(browser, { url: empty.url, scheme, viewport: { width: 1280, height: 900 }, route: "#/projects", label: "projects-empty" });
    }
  }
} finally {
  await browser.close();
  for (const server of servers) server.stop();
}

if (failures.length) {
  console.error(`${failures.length} problem(s):`);
  for (const failure of [...new Set(failures)]) console.error(`  ${failure}`);
  process.exit(1);
}
console.log("Every console view rendered in light and dark without errors.");
