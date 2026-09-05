// Renders the built-in MCP catalog as data the website can list.
//
// The catalog has one home, `crates/tuff-core/assets/mcp-catalog.toml`,
// which is compiled into the binary with `include_str!`. The website never
// carries a second copy that can drift: this script runs before every
// `astro check`, `astro build`, and `astro dev` (see the pre-scripts in
// package.json) and writes src/data/mcp-catalog.json, which is gitignored.
//
// It also validates. A catalog entry that would not resolve in the CLI
// fails the site build here rather than shipping a listing that promises
// something `tuff add mcp` cannot deliver.

import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { parse } from "smol-toml";

const here = dirname(fileURLToPath(import.meta.url));
const source = resolve(here, "../../crates/tuff-core/assets/mcp-catalog.toml");
const target = resolve(here, "../src/data/mcp-catalog.json");

const catalog = parse(readFileSync(source, "utf8"));
const servers = catalog.servers ?? [];
if (!Array.isArray(servers) || servers.length === 0) {
  throw new Error(`${source} declares no servers`);
}

/** How the server is started, which is what a reader is really choosing between. */
function launcher(server) {
  if (server.transport === "http") {
    return "remote";
  }
  switch (server.command) {
    case "npx":
      return "npm";
    case "uvx":
      return "python";
    case "docker":
      return "docker";
    default:
      return server.command ?? "local";
  }
}

/**
 * Every environment variable the entry expects the developer to export,
 * across `env` and the `from_env` references inside `headers`. This mirrors
 * `catalog::required_env` in tuff-core, which drives the CLI's own
 * post-install reminder.
 */
function variables(server) {
  const fromEnv = server.env ?? [];
  const fromHeaders = Object.values(server.headers ?? {}).map((header) => header.from_env);
  return [...new Set([...fromEnv, ...fromHeaders])].sort();
}

const entries = servers.map((server) => {
  for (const field of ["id", "version", "description"]) {
    if (typeof server[field] !== "string" || server[field].length === 0) {
      throw new Error(`catalog entry ${server.id ?? "(unnamed)"} has no ${field}`);
    }
  }
  const transport = server.transport ?? "stdio";
  if (transport === "http") {
    if (typeof server.url !== "string") {
      throw new Error(`catalog entry '${server.id}' is http but declares no url`);
    }
  } else if (typeof server.command !== "string") {
    throw new Error(`catalog entry '${server.id}' is stdio but declares no command`);
  }
  for (const [name, header] of Object.entries(server.headers ?? {})) {
    if (typeof header?.from_env !== "string") {
      throw new Error(
        `catalog entry '${server.id}' header ${name} must reference a variable, never a value`,
      );
    }
  }

  const needed = variables(server);
  return {
    id: server.id,
    version: server.version,
    description: server.description,
    transport,
    launcher: launcher(server),
    // The command as the harness would run it, for readers comparing an
    // entry against the vendor's own README.
    command:
      transport === "http"
        ? server.url
        : [server.command, ...(server.args ?? [])].join(" "),
    variables: needed,
    needsKey: needed.length > 0,
    tools: server.tools_summary
      ? server.tools_summary.split(",").map((tool) => tool.trim()).filter(Boolean)
      : [],
  };
});

const ids = entries.map((entry) => entry.id);
if (new Set(ids).size !== ids.length) {
  throw new Error("the catalog declares the same id twice");
}

mkdirSync(dirname(target), { recursive: true });
writeFileSync(target, `${JSON.stringify({ entries }, null, 2)}\n`);
