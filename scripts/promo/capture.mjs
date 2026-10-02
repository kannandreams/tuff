// Screenshots every Tuff Console view from `tuff console serve --demo` at 2x.
//   PW=<path to playwright> node capture.mjs <tuff binary> <out dir>
import { spawn } from "node:child_process";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PW);
const [bin, out] = process.argv.slice(2);
const child = spawn(bin, ["console", "serve", "--demo", "--addr", "127.0.0.1:0"], { stdio: ["ignore", "pipe", "inherit"] });
const url = await new Promise((done) => { let b = ""; child.stdout.on("data", (c) => { b += c; const m = b.match(/Console listening on (http:\/\/\S+)/); if (m) done(m[1]); }); });
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 2, colorScheme: "light" });
for (const [name, hash] of [["dashboard", "#/"], ["projects", "#/projects"], ["capabilities", "#/capabilities"], ["harnesses", "#/harnesses"], ["policies", "#/policies"], ["audit", "#/audit"]]) {
  await page.goto(url + "/" + hash); await page.waitForLoadState("networkidle");
  await page.evaluate(() => document.fonts.ready);
  await page.evaluate(() => Promise.all(document.getAnimations().map((a) => a.finished)));
  await page.waitForTimeout(300);
  await page.screenshot({ path: `${out}/${name}.png` });
}
await browser.close(); child.kill();
