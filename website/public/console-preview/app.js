"use strict";
/* Tuff Console UI. Plain JavaScript over the JSON API under /api/v1.
   Views are linkable: the route lives in the URL hash. Every value that
   reaches the page goes through h`...`, which escapes it. */

const HNAME = {
  claude: "Claude Code", codex: "Codex", cursor: "Cursor", opencode: "OpenCode", "open-agents": "Open Agents",
};
const DOCS = "https://tuffcli.dev/cli/console/";
const hname = (h) => HNAME[h] || h;

/* ── escaping ─────────────────────────────────────────────────────── */
class Raw { constructor(text) { this.text = text; } }
const raw = (text) => new Raw(text);
const ESC = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };
function toHtml(value) {
  if (value instanceof Raw) return value.text;
  if (Array.isArray(value)) return value.map(toHtml).join("");
  if (value === null || value === undefined || value === false) return "";
  return String(value).replace(/[&<>"']/g, (c) => ESC[c]);
}
function h(strings, ...values) {
  let out = strings[0];
  values.forEach((value, i) => { out += toHtml(value) + strings[i + 1]; });
  return new Raw(out);
}

/* ── small helpers ────────────────────────────────────────────────── */
const $ = (selector) => document.querySelector(selector);
const plural = (n, one, many) => `${n} ${n === 1 ? one : many || one + "s"}`;
const isOk = (status) => status === "ok" || status === "unknown";

function ago(iso) {
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return "";
  const seconds = Math.max(0, (Date.now() - t) / 1000);
  const minutes = Math.floor(seconds / 60);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.floor(hours / 24);
  if (days === 1) return "yesterday";
  if (days < 45) return `${days} days ago`;
  return new Date(t).toISOString().slice(0, 10);
}
const when = (iso) => h`<time datetime="${iso}" title="${iso}">${ago(iso)}</time>`;
const short = (sha) => (sha ? String(sha).slice(0, 7) : "");

const link = (hash, label, cls) => h`<a href="${hash}" class="${cls || ""}">${label}</a>`;
const projectLink = (id, label) => link(`#/projects/${id}`, label, "rowlink");
const capabilityHash = (type, id) => `#/capabilities/${encodeURIComponent(type)}/${encodeURIComponent(id)}`;

const tags = (list) => list.map((item) => h`<span class="tag">${hname(item)}</span>`);
/* A path may wrap after each "/" and nowhere inside a name. */
const breakable = (text) => raw(toHtml(text).replace(/\//g, "/<wbr>"));
const projectPath = (p) => breakable(p.path && p.path !== "." ? `${p.repository} · ${p.path}` : p.repository);

function projectPill(status) {
  const map = {
    ok: ["ok", "ok"], drift: ["bad", "drift"], gap: ["warn", "policy gap"], outdated: ["warn", "outdated"],
  };
  const [cls, label] = map[status] || ["mute", status];
  return h`<span class="pill ${cls}">${label}</span>`;
}

function rowPills(row) {
  const pills = [];
  if (row.status === "unknown") pills.push(h`<span class="pill mute">not checked</span>`);
  else if (!isOk(row.status)) pills.push(h`<span class="pill bad">${row.status}</span>`);
  if (row.outdated) pills.push(h`<span class="pill warn">outdated</span>`);
  if (!pills.length) pills.push(h`<span class="pill ok">ok</span>`);
  return pills;
}

function panel(title, meta, body) {
  return h`<section class="panel"><div class="panel-h"><h2>${title}</h2>${meta ? h`<span class="meta">${meta}</span>` : ""}</div>${body}</section>`;
}
const scrollTable = (inner) => h`<div class="scroll"><table>${inner}</table></div>`;
/* A table whose rows become stacked cards at phone width; each cell
   carries its column name in data-label. */
const cardTable = (inner) => h`<div class="scroll"><table class="cards">${inner}</table></div>`;
const note = (text) => h`<p class="note">${text}</p>`;
const heading = (title, sub, aside) => h`<div class="head"><div><h1>${title}</h1>${sub ? h`<p class="sub">${sub}</p>` : ""}</div>${aside || ""}</div>`;

function emptyConsole() {
  return h`<div class="empty">
    <h2>No reports yet</h2>
    <p>Each project sends a report with the command below. A project appears here after its first accepted report. <b>--all</b> reports every folder with a tuff.lock under the current one, so each app in a monorepo becomes its own project.</p>
    <pre class="cmd"><span class="p">$ </span>tuff console publish --server ${location.origin}</pre>
    <p>From GitHub Actions the job publishes without a secret. The <a href="${DOCS}" rel="noreferrer">console documentation</a> shows the workflow and the API key setup for other CI systems.</p>
  </div>`;
}

/* ── events ───────────────────────────────────────────────────────── */
const EVENT_MARK = {
  drift_detected: "bad", policy_gap_added: "warn", capability_removed: "warn", target_removed: "warn",
  drift_cleared: "ok", policy_gap_closed: "ok",
  version_changed: "act", capability_added: "act", target_added: "act", project_first_seen: "",
};

function eventText(e) {
  const id = h`<b>${e.capabilityId}</b>`;
  const on = e.target ? h` on ${e.target.split(", ").map(hname).join(", ")}` : "";
  switch (e.kind) {
    case "capability_added": return h`${id} added${e.capabilityType ? h` (${e.capabilityType}${e.detail ? " " + e.detail : ""})` : ""}${on}`;
    case "capability_removed": return h`${id} removed${on}`;
    case "version_changed": return h`${id} ${e.detail || "changed"}${on}`;
    case "target_added": return h`${id} now installed${e.target ? h` for ${hname(e.target)}` : ""}`;
    case "target_removed": return h`${id} no longer installed${e.target ? h` for ${hname(e.target)}` : ""}`;
    case "drift_detected": return h`${id} differs from tuff.lock${e.detail ? h` (${e.detail})` : ""}${on}`;
    case "drift_cleared": return h`${id} matches tuff.lock again${on}`;
    case "policy_gap_added": return h`${id} ${e.detail || "rule"} recorded as not enforced${on}`;
    case "policy_gap_closed": return h`${id} ${e.detail || "rule"} is enforced again${on}`;
    case "project_first_seen": return h`First report from <span class="mono">${e.repository}${e.path && e.path !== "." ? " " + e.path : ""}</span>`;
    default: return h`${e.kind}`;
  }
}

function eventList(events, showProject = true) {
  if (!events.length) return note("No events match.");
  return h`<ol class="events">${events.map((e) => h`
    <li class="ev">${when(e.occurredAt)}<div class="what"><span class="mark ${EVENT_MARK[e.kind] || ""}" aria-hidden="true"></span>${eventText(e)}
      <div class="where">${showProject ? h`${projectLink(e.projectId, e.projectName || "project " + e.projectId)} · ` : ""}<span class="mono" data-kind="${e.kind}">${e.kind}</span>${e.commit ? h` · commit <span class="mono">${short(e.commit)}</span>` : ""}</div></div></li>`)}</ol>`;
}

/* ── views ────────────────────────────────────────────────────────── */
async function dashboardView(ctx) {
  const { projects, summary } = ctx.projects;
  if (!projects.length) return h`${heading("Dashboard", "Every project that has published a report to this console, and what changed recently.")}${emptyConsole()}`;
  const { events } = await api("/events?limit=6");
  const attention = projects.filter((p) => p.status !== "ok");
  const why = (p) => {
    const parts = [];
    if (p.driftCount) parts.push(`${plural(p.driftCount, "capability", "capabilities")} edited by hand`);
    if (p.policyGapCount) parts.push(`${plural(p.policyGapCount, "policy rule")} not enforced`);
    if (p.outdatedCount) parts.push(`${plural(p.outdatedCount, "capability", "capabilities")} behind the newest version`);
    return parts.join("; ");
  };
  return h`
  ${heading("Dashboard", "Every project that has published a report to this console, and what changed recently.")}
  <section class="stats" aria-label="Summary">
    <div class="stat"><div class="v">${summary.projects}</div><div class="l">${plural(summary.projects, "project")} in ${plural(summary.repositories, "repository", "repositories")}</div></div>
    <div class="stat"><div class="v">${summary.capabilities}</div><div class="l">distinct capabilities</div></div>
    <div class="stat"><div class="v">${summary.harnesses}</div><div class="l">harnesses in use</div></div>
    <div class="stat ${summary.driftCount ? "bad" : ""}"><div class="v">${summary.driftCount}</div><div class="l">edited by hand (drift)</div></div>
    <div class="stat ${summary.outdatedCount ? "warn" : ""}"><div class="v">${summary.outdatedCount}</div><div class="l">behind the newest version</div></div>
    <div class="stat ${summary.policyGapCount ? "warn" : ""}"><div class="v">${summary.policyGapCount}</div><div class="l">policy rules not enforced</div></div>
  </section>
  <div class="grid2">
    ${panel("Needs attention", `${attention.length} of ${projects.length} projects`, attention.length
      ? scrollTable(h`<thead><tr><th>Project</th><th>Status</th><th>Why</th></tr></thead><tbody>${attention.map((p) => h`
        <tr class="clickable" data-href="#/projects/${p.id}"><td>${projectLink(p.id, p.name)}<div class="path">${projectPath(p)}</div></td><td>${projectPill(p.status)}</td><td class="dim">${why(p)}</td></tr>`)}</tbody>`)
      : note("Every project matches its lockfile, is on the newest version it was checked against, and enforces every policy rule."))}
    ${panel("Recent changes", "", h`${eventList(events)}<div class="panel-h">${link("#/audit", "All events", "iconbtn")}</div>`)}
  </div>
  <div class="callout">
    <p>To add a project, run publish in its folder. <b>--all</b> sends one report for every folder with a tuff.lock, so each app in a monorepo shows up as its own project.</p>
    <pre class="cmd"><span class="p">$ </span>tuff console publish --all</pre>
  </div>`;
}

function projectsView(ctx) {
  const { projects } = ctx.projects;
  if (!projects.length) return h`${heading("Projects", "A project is one folder with a tuff.lock.")}${emptyConsole()}`;
  return h`${heading("Projects", "A project is one folder with a tuff.lock. A monorepo publishes one project per app.")}
  <section class="panel">${scrollTable(h`<thead><tr><th>Project</th><th>Harnesses</th><th class="num">Capabilities</th><th>Last report</th><th>Status</th></tr></thead>
    <tbody>${projects.map((p) => h`<tr class="clickable" data-href="#/projects/${p.id}">
      <td>${projectLink(p.id, p.name)}<div class="path">${projectPath(p)}</div></td>
      <td>${tags(p.harnesses)}</td><td class="num">${p.capabilityCount}</td>
      <td><span class="tabnum">${when(p.lastReportAt)}</span><div class="path">${p.branch || "no branch"}${p.commit ? " @ " + short(p.commit) : ""}${p.dirty ? " · uncommitted changes" : ""}</div></td>
      <td>${projectPill(p.status)}</td></tr>`)}</tbody>`)}</section>`;
}

/** Rows with the same state collapse into one row with several harnesses. */
function groupCapabilityRows(rows) {
  const groups = new Map();
  for (const row of rows) {
    const key = [row.type, row.id, row.version, row.status, row.outdated, row.latest, row.source].join("\u0000");
    if (!groups.has(key)) groups.set(key, { ...row, targets: [] });
    groups.get(key).targets.push(row.target);
  }
  return [...groups.values()];
}

async function projectView(ctx, segments) {
  const id = Number(segments[1]);
  const [detail, history, timeline] = await Promise.all([
    api(`/projects/${id}`), api(`/projects/${id}/reports`), api(`/events?project=${id}&limit=30`),
  ]);
  const p = detail.project;
  const rows = groupCapabilityRows(detail.capabilities);
  return h`
  <div><div class="crumb">${link("#/projects", "Projects")} / ${p.name}</div>
  <div class="head"><div><h1>${p.name}</h1><p class="sub mono">${projectPath(p)}</p></div>${projectPill(p.status)}</div></div>
  <div class="grid2">
    ${panel("Capabilities", `from tuff.lock${p.commit ? " at " + short(p.commit) : ""}`, cardTable(h`
      <thead><tr><th>Capability</th><th>Type</th><th>Version</th><th>Harnesses</th><th>Status</th></tr></thead>
      <tbody>${rows.map((r) => h`<tr><td class="card-title">${link(capabilityHash(r.type, r.id), r.id, "rowlink nowrap")}</td><td data-label="Type"><span class="kind">${r.type}</span></td>
        <td data-label="Version" class="tabnum">${r.version}${r.outdated && r.latest ? h`<div class="path">newest ${r.latest}</div>` : ""}</td>
        <td data-label="Harnesses">${tags(r.targets)}</td><td data-label="Status">${rowPills(r)}</td></tr>`)}</tbody>`))}
    <div class="stack">
      ${panel("Latest report", "", h`<dl class="kv">
        <dt>Received</dt><dd>${when(p.lastReportAt)}</dd>
        <dt>Branch</dt><dd class="mono">${p.branch || "none"}</dd>
        <dt>Commit</dt><dd class="mono">${p.commit || "none"}</dd>
        <dt>Working tree</dt><dd>${p.dirty ? "uncommitted changes" : "clean"}</dd>
        <dt>Harnesses</dt><dd>${p.harnesses.map(hname).join(", ")}</dd>
        <dt>Reports stored</dt><dd>${p.reportCount}</dd></dl>`)}
      ${detail.policyGaps.length ? panel("Policy rules not enforced", "", scrollTable(h`<tbody>${detail.policyGaps.map((g) => h`
        <tr><td><b>${g.policy}</b> rule ${g.rule} on ${hname(g.target)}<div class="path">${g.description}</div></td><td class="dim">${g.reason}</td></tr>`)}</tbody>`)) : ""}
      ${panel("Timeline", "", eventList(timeline.events, false))}
      ${panel("Report history", `${history.reports.length} stored`, scrollTable(h`
        <thead><tr><th>Received</th><th>Commit</th><th>Branch</th><th>Tuff</th></tr></thead>
        <tbody>${history.reports.slice(0, 10).map((r) => h`<tr><td class="tabnum">${when(r.receivedAt)}</td><td class="mono">${short(r.commit)}</td><td class="mono">${r.branch || ""}</td><td class="mono">${r.tuffVersion}</td></tr>`)}</tbody>`))}
    </div>
  </div>`;
}

function versionTags(versions) {
  return versions.map((v) => h`<span class="tag tabnum">${v.version} × ${v.projects}</span>`);
}

async function capabilitiesView(ctx, segments, query) {
  const type = query.get("type") || "";
  const data = await api(`/capabilities${type ? "?type=" + encodeURIComponent(type) : ""}`);
  const all = data;
  const chip = (value, label) => h`<a class="filter" href="#/capabilities${value ? "?type=" + encodeURIComponent(value) : ""}" aria-current="${type === value}">${label}</a>`;
  if (!ctx.projects.projects.length) return h`${heading("Capabilities", "Every capability across projects, with the versions in use.")}${emptyConsole()}`;
  return h`
  ${heading("Capabilities", "Every capability across projects, with the versions in use.",
    h`<div class="filters" role="group" aria-label="Filter by type">${chip("", "All types")}${all.types.map((t) => chip(t, t))}</div>`)}
  <section class="panel">${data.capabilities.length ? scrollTable(h`
    <thead><tr><th>Capability</th><th>Type</th><th>Versions in use</th><th class="num">Projects</th><th>Used in</th></tr></thead>
    <tbody>${data.capabilities.map((c) => h`<tr class="clickable" data-href="${capabilityHash(c.type, c.id)}"><td>${link(capabilityHash(c.type, c.id), c.id, "rowlink")}</td><td><span class="kind">${c.type}</span></td>
      <td>${versionTags(c.versions)}${c.mixed ? h` <span class="pill warn">mixed</span>` : ""}</td>
      <td class="num">${c.projectCount}</td><td class="dim">${c.projects.map((p) => p.name).join(", ")}</td></tr>`)}</tbody>`) : note("No capability of this type is in use.")}</section>`;
}

async function capabilityView(ctx, segments) {
  const [type, id] = [segments[1], segments[2]];
  const [detail, timeline] = await Promise.all([
    api(`/capabilities/${encodeURIComponent(type)}/${encodeURIComponent(id)}`),
    api(`/events?capability=${encodeURIComponent(id)}&limit=30`),
  ]);
  const events = timeline.events.filter((e) => e.capabilityType === type);
  return h`
  <div><div class="crumb">${link("#/capabilities", "Capabilities")} / ${id}</div>
  <div class="head"><div><h1>${id}</h1><p class="sub"><span class="kind">${type}</span> used in ${plural(detail.projectCount, "project")}</p></div>
    <div>${versionTags(detail.versions)}${detail.mixed ? h` <span class="pill warn">mixed</span>` : ""}</div></div></div>
  <div class="grid2">
    ${panel("Where it is used", "", scrollTable(h`
      <thead><tr><th>Project</th><th>Harness</th><th>Version</th><th>Status</th></tr></thead>
      <tbody>${detail.usage.map((u) => h`<tr><td>${projectLink(u.projectId, u.name)}<div class="path">${projectPath(u)}</div></td>
        <td>${hname(u.target)}</td><td class="tabnum">${u.version}${u.outdated && u.latest ? h`<div class="path">newest ${u.latest}</div>` : ""}<div class="path">${breakable(u.source)}</div></td><td>${rowPills(u)}</td></tr>`)}</tbody>`))}
    ${panel("History", "", eventList(events))}
  </div>`;
}

function harnessesView(ctx, segments, query, data) {
  const { harnesses, projects, totals } = data;
  const title = "Which projects install capabilities for which harness. A number is the count of capabilities installed there.";
  if (!projects.length) return h`${heading("Harnesses", title)}${emptyConsole()}`;
  const cell = (p, x) => (p.counts[x]
    ? h`<td><span class="cell on">${p.counts[x]}</span></td>`
    : h`<td><span class="cell off" aria-label="none">·</span></td>`);
  return h`${heading("Harnesses", title)}
  <section class="panel">${scrollTable(h`<thead><tr><th>Project</th>${harnesses.map((x) => h`<th>${hname(x)}</th>`)}</tr></thead>
    <tbody>${projects.map((p) => h`<tr class="clickable" data-href="#/projects/${p.id}"><td>${projectLink(p.id, p.name)}<div class="path">${projectPath(p)}</div></td>${harnesses.map((x) => cell(p, x))}</tr>`)}
    <tr><td class="dim">Projects</td>${harnesses.map((x) => h`<td class="tabnum dim">${totals[x]}</td>`)}</tr></tbody>`)}</section>`;
}

function policyPanel(policy) {
  const versions = policy.versions.map((v) => `${v.version} × ${v.projects}`).join(", ");
  const meta = h`${policy.mixed ? h`<span class="pill warn">mixed</span> ` : ""}${versions}`;
  const row = (u) => h`<tr><td>${projectLink(u.id, u.name)}${u.gapCount ? h` <span class="pill warn">${plural(u.gapCount, "gap")}</span>` : ""}</td>
    <td class="tabnum">${u.version}${u.outdated ? h` <span class="pill warn">outdated</span>${u.latest ? h`<div class="path">newest ${u.latest}</div>` : ""}` : ""}</td>
    <td>${tags(u.targets)}</td></tr>`;
  return panel(policy.id, meta, scrollTable(h`<thead><tr><th>Project</th><th>Version</th><th>Harnesses</th></tr></thead><tbody>${policy.usage.map(row)}</tbody>`));
}

function gapsTable(gaps) {
  if (!gaps.length) return note("Every rule of every installed policy is enforced.");
  const row = (g) => h`<tr class="clickable" data-href="#/projects/${g.projectId}"><td>${projectLink(g.projectId, g.name)}</td>
    <td><b>${g.policy}</b> rule ${g.rule}<div class="path">${g.description}</div></td><td>${hname(g.target)}</td><td class="dim">${g.reason}</td></tr>`;
  return scrollTable(h`<thead><tr><th>Project</th><th>Policy and rule</th><th>Harness</th><th>Reason</th></tr></thead><tbody>${gaps.map(row)}</tbody>`);
}

function policiesView(ctx, segments, query, data) {
  const title = "Which projects carry which policies, and every rule a harness does not enforce.";
  if (!ctx.projects.projects.length) return h`${heading("Policies", title)}${emptyConsole()}`;
  const without = data.projectsWithoutPolicy;
  const carriers = data.policies.length
    ? h`<div class="stack">${data.policies.map(policyPanel)}</div>`
    : panel("Policies", "", note("No project has installed a policy."));
  const missing = panel("Projects without a policy", String(without.length), without.length
    ? scrollTable(h`<tbody>${without.map((p) => h`<tr><td>${projectLink(p.id, p.name)}<div class="path">${projectPath(p)}</div></td><td>${tags(p.harnesses)}</td></tr>`)}</tbody>`)
    : note("Every project carries a policy."));
  return h`${heading("Policies", title)}
  ${panel("Rules not enforced", "recorded with --accept-unenforced; tuff check --strict fails on these", gapsTable(data.gaps))}
  <div class="grid2">${carriers}${missing}</div>`;
}

/* Events per Audit page; "Older events" pages back by event id. */
const AUDIT_PAGE = 50;

async function auditView(ctx, segments, query) {
  const params = new URLSearchParams();
  for (const key of ["project", "capability", "kind", "since"]) if (query.get(key)) params.set(key, query.get(key));
  if (query.get("before")) params.set("before", query.get("before"));
  params.set("limit", String(AUDIT_PAGE));
  const { events, kinds, nextBefore } = await api(`/events?${params}`);
  const pageHash = (before) => {
    const next = new URLSearchParams(query);
    if (before) next.set("before", before); else next.delete("before");
    const text = next.toString();
    return `#/audit${text ? "?" + text : ""}`;
  };
  const pager = query.get("before") || nextBefore
    ? h`<div class="pager">${query.get("before") ? link(pageHash(null), "Newest events", "iconbtn") : ""}${nextBefore ? link(pageHash(nextBefore), "Older events", "iconbtn") : ""}</div>`
    : "";
  const { projects } = ctx.projects;
  const selected = (key, value) => (query.get(key) === String(value) ? "selected" : "");
  return h`${heading("Audit", "Computed from consecutive reports of each project. Every event names the commit it came from.")}
  <section class="panel">
    <div class="fields" id="audit-filters">
      <label class="field">Project<select data-filter="project"><option value="">All projects</option>${projects.map((p) => h`<option value="${p.id}" ${raw(selected("project", p.id))}>${p.name} (${p.repository})</option>`)}</select></label>
      <label class="field">Event<select data-filter="kind"><option value="">All events</option>${kinds.map((k) => h`<option value="${k}" ${raw(selected("kind", k))}>${k.replace(/_/g, " ")}</option>`)}</select></label>
      <label class="field">Capability<input type="text" data-filter="capability" value="${query.get("capability") || ""}" placeholder="capability id"></label>
      <label class="field">Since<input type="date" data-filter="since" value="${query.get("since") || ""}"></label>
    </div>
    ${events.length ? eventList(events) : note("No events match these filters.")}
    ${pager}
  </section>`;
}

function settingsView(ctx) {
  const s = ctx.settings;
  const access = s.server.loopback ? "local, only this machine can connect" : "reachable from the network";
  return h`${heading("Settings", "Read only. Trusts and keys are set with tuff console serve and tuff console key on the host that runs the console.")}
  <div class="grid2">
    ${s.server.preview ? panel("Server", "", note("This preview runs in your browser with sample data. A console you run shows its address, who can reach it, and whether publishing needs a key.")) : h`${panel("Server", "", h`<dl class="kv">
      <dt>Address</dt><dd class="mono">${s.server.address || ""}</dd>
      <dt>Access</dt><dd>${access}</dd>
      <dt>Viewing</dt><dd>${s.server.loopback ? "no sign-in" : (s.server.publicRead ? "no sign-in; put a reverse proxy that authenticates people in front" : "no sign-in")}</dd>
      <dt>Publishing</dt><dd>${s.server.publishRequiresAuth ? "needs a key or a trusted GitHub Actions token" : "open to anyone who can connect"}</dd>
      <dt>Data</dt><dd>${s.server.demo ? "generated sample data" : "console.sqlite"}</dd>
      <dt>Version</dt><dd class="mono">${s.server.version}</dd></dl>`)}`}
    <div class="stack">
      ${panel("Trusts", "GitHub Actions", s.trusts.length
        ? scrollTable(h`<thead><tr><th>Provider</th><th>Owner</th></tr></thead><tbody>${s.trusts.map((t) => h`<tr><td>${t.provider}</td><td class="mono">${t.owner}</td></tr>`)}</tbody>`)
        : note("None. Start the console with --trust github:<owner> to accept publishing from that owner's GitHub Actions jobs."))}
      ${s.server.audience ? panel("OIDC audience", "", h`<p class="note mono">${s.server.audience}</p>`) : ""}
      ${panel("API keys", "names only", s.keys.length
        ? scrollTable(h`<thead><tr><th>Name</th><th>Repository</th><th>Created</th><th>Last used</th></tr></thead><tbody>${s.keys.map((k) => h`
          <tr><td class="mono">${k.name}</td><td class="path">${k.repository ? breakable(k.repository) : "any"}</td><td>${when(k.createdAt)}</td><td>${k.lastUsedAt ? when(k.lastUsedAt) : "never"}</td></tr>`)}</tbody>`)
        : note("None. Create one with tuff console key create <name>."))}
    </div>
  </div>`;
}

/* ── data and routing ─────────────────────────────────────────────── */
async function api(path) {
  const response = await fetch(`/api/v1${path}`, { headers: { Accept: "application/json" } });
  let body = null;
  try { body = await response.json(); } catch (_) { /* no body */ }
  if (!response.ok) {
    const error = body && body.error;
    throw new Error(error ? `${error.message}${error.hint ? " (" + error.hint + ")" : ""}` : `HTTP ${response.status}`);
  }
  return body;
}

const NAV = [
  ["#/", "Dashboard", "dashboard"], ["#/projects", "Projects", "projects"], ["#/capabilities", "Capabilities", "capabilities"],
  ["#/harnesses", "Harnesses", "harnesses"], ["#/policies", "Policies", "policies"], ["#/audit", "Audit", "audit"],
  ["#/settings", "Settings", "settings"],
];

function parseRoute() {
  const [path, search] = location.hash.replace(/^#/, "").split("?");
  const segments = path.split("/").filter(Boolean).map((part) => { try { return decodeURIComponent(part); } catch (_) { return part; } });
  return { segments, query: new URLSearchParams(search || "") };
}

function viewName(segments) { return segments[0] || "dashboard"; }

function renderChrome(ctx, current) {
  const s = ctx.settings.server;
  const access = s.loopback
    ? "local, only this machine can connect"
    : (s.publishRequiresAuth ? "publishing requires authentication" : "publishing is not authenticated");
  /* The website's in-browser preview sets `preview` and has no address to show. */
  $("#server").innerHTML = s.preview ? "" : toHtml(h`<span class="dot" aria-hidden="true"></span><span class="mono">${s.address || location.host}</span><span class="hide-sm">· ${access}</span>`);
  $("#demo-chip").hidden = !s.demo;
  const last = ctx.projects.summary.lastReportAt;
  $("#last-report").textContent = last ? `Last report ${ago(last)}` : "No reports yet";
  const sum = ctx.projects.summary;
  const counts = { projects: sum.projects, capabilities: sum.capabilities, policies: sum.policyGapCount };
  $("#nav").innerHTML = toHtml(h`${NAV.map(([hash, label, id]) => {
    const n = counts[id];
    const cls = id === "policies" && n ? "count alert" : "count";
    return h`<a href="${hash}" ${raw(current === id ? 'aria-current="page"' : "")}>${label}${n ? h`<span class="${cls}">${n}</span>` : ""}</a>`;
  })}<hr><p class="hint">Reports arrive when a project runs<br><code>tuff console publish</code></p>`);
}

let renderToken = 0;
async function render() {
  const token = ++renderToken;
  const { segments, query } = parseRoute();
  const name = viewName(segments);
  const main = $("#main");
  try {
    const [settings, projects] = await Promise.all([api("/settings"), api("/projects")]);
    const ctx = { settings, projects };
    if (token !== renderToken) return;
    renderChrome(ctx, name);
    let view;
    if (name === "dashboard") view = await dashboardView(ctx);
    else if (name === "projects") view = segments[1] ? await projectView(ctx, segments) : projectsView(ctx);
    else if (name === "capabilities") view = segments.length >= 3 ? await capabilityView(ctx, segments) : await capabilitiesView(ctx, segments, query);
    else if (name === "harnesses") view = harnessesView(ctx, segments, query, await api("/harnesses"));
    else if (name === "policies") view = policiesView(ctx, segments, query, await api("/policies"));
    else if (name === "audit") view = await auditView(ctx, segments, query);
    else if (name === "settings") view = settingsView(ctx);
    else view = h`${heading("Not found", "")}<p class="note">${link("#/", "Back to the Dashboard")}</p>`;
    if (token !== renderToken) return;
    main.innerHTML = toHtml(h`<div class="view">${view}</div>`);
    document.title = `${name === "dashboard" ? "Dashboard" : name[0].toUpperCase() + name.slice(1)} - Tuff Console`;
  } catch (error) {
    if (token !== renderToken) return;
    main.innerHTML = toHtml(h`<div class="view">${heading("Something went wrong", "")}<div class="error" role="alert">${error.message}</div></div>`);
  }
}

let lastSegment = parseRoute().segments.join("/");
window.addEventListener("hashchange", async () => {
  const segment = parseRoute().segments.join("/");
  const moved = segment !== lastSegment;
  lastSegment = segment;
  await render();
  // A filter change keeps the scroll position, a move to another view starts at the top.
  if (moved) window.scrollTo(0, 0);
});

document.addEventListener("click", (event) => {
  const row = event.target.closest("tr[data-href]");
  if (row && !event.target.closest("a")) location.hash = row.dataset.href;
});

document.addEventListener("change", (event) => {
  const control = event.target.closest("[data-filter]");
  if (!control) return;
  const { query } = parseRoute();
  if (control.value) query.set(control.dataset.filter, control.value); else query.delete(control.dataset.filter);
  query.delete("before");
  const text = query.toString();
  location.hash = `#/audit${text ? "?" + text : ""}`;
});

function setTheme(theme) {
  document.documentElement.dataset.theme = theme;
  try { localStorage.setItem("tuff-console-theme", theme); } catch (_) { /* storage is optional */ }
}
$("#theme-btn").addEventListener("click", () => {
  const root = document.documentElement;
  const dark = root.dataset.theme ? root.dataset.theme === "dark" : matchMedia("(prefers-color-scheme: dark)").matches;
  setTheme(dark ? "light" : "dark");
});
try {
  const saved = localStorage.getItem("tuff-console-theme");
  if (saved === "light" || saved === "dark") document.documentElement.dataset.theme = saved;
} catch (_) { /* storage is optional */ }

render();
