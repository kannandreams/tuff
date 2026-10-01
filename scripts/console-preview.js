"use strict";
/* The website's Tuff Console preview: answers the console UI's /api/v1
   requests from data.json, a snapshot of `tuff console serve --demo`
   written by scripts/build-console-preview.mjs. Times in the snapshot are
   moved forward by the time since it was taken, so "4 min ago" stays true.
   Events are filtered and paged here the way GET /api/v1/events does. */
(function () {
  const ISO = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?Z$/;
  const shift = (value, offset) => {
    if (typeof value === "string") return ISO.test(value) ? new Date(Date.parse(value) + offset).toISOString() : value;
    if (Array.isArray(value)) return value.map((item) => shift(item, offset));
    if (value && typeof value === "object") {
      const out = {};
      for (const key of Object.keys(value)) out[key] = shift(value[key], offset);
      return out;
    }
    return value;
  };

  const snapshot = fetch("data.json")
    .then((response) => response.json())
    .then((data) => shift(data, Math.max(0, Date.now() - Date.parse(data.takenAt))));

  const reply = (status, body) =>
    new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });

  function events(data, query) {
    const limit = Math.min(Math.max(Number(query.get("limit")) || 200, 1), 1000);
    const project = query.get("project");
    const capability = query.get("capability");
    const kind = query.get("kind");
    const since = query.get("since");
    const before = query.get("before");
    const list = data.events
      .filter((e) => !project || String(e.projectId) === project)
      .filter((e) => !capability || e.capabilityId === capability)
      .filter((e) => !kind || e.kind === kind)
      .filter((e) => !since || e.occurredAt >= since)
      .filter((e) => !before || e.id < Number(before))
      .slice(0, limit);
    const nextBefore = list.length === limit ? list[list.length - 1].id : null;
    return { events: list, kinds: data.kinds, nextBefore };
  }

  const realFetch = window.fetch.bind(window);
  window.fetch = async function (input, init) {
    const url = new URL(typeof input === "string" ? input : input.url, location.href);
    if (!url.pathname.startsWith("/api/v1/")) return realFetch(input, init);
    const data = await snapshot;
    const path = url.pathname.slice("/api/v1".length);
    if (path === "/events") return reply(200, events(data, url.searchParams));
    const key = path + url.search;
    if (key in data.responses) return reply(200, data.responses[key]);
    return reply(404, { error: { kind: "not_found", message: "This preview has no data for that page." } });
  };

  /* The landing page is light; a viewer's own choice in the preview wins. */
  try {
    if (!localStorage.getItem("tuff-console-theme")) document.documentElement.dataset.theme = "light";
  } catch (_) {
    document.documentElement.dataset.theme = "light";
  }
})();
