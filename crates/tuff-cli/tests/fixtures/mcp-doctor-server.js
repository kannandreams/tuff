#!/usr/bin/env node
// A minimal, spec-conformant (newline-delimited JSON-RPC) stdio MCP server,
// used only by `tuff mcp doctor`'s CLI tests -- deliberately local and
// network-free so the test suite never depends on npx/network access.
//
// Set MCP_DOCTOR_TEST_DELAY_MS to sleep before responding to any request,
// exercising doctor's --timeout handling.

const delayMs = Number.parseInt(process.env.MCP_DOCTOR_TEST_DELAY_MS || "0", 10);

function send(message) {
  process.stdout.write(JSON.stringify(message) + "\n");
}

function respond(id, result) {
  send({ jsonrpc: "2.0", id, result });
}

async function handle(message) {
  if (!message || typeof message !== "object" || message.id === undefined) {
    return;
  }
  if (delayMs > 0) {
    await new Promise((resolve) => setTimeout(resolve, delayMs));
  }
  if (message.method === "initialize") {
    respond(message.id, {
      protocolVersion: "2024-11-05",
      capabilities: { tools: {} },
      serverInfo: { name: "mcp-doctor-test-server", version: "0.1.0" },
    });
  } else if (message.method === "tools/list") {
    respond(message.id, {
      tools: [
        { name: "echo", description: "Echo input back." },
        { name: "ping", description: "Return pong." },
      ],
    });
  } else {
    send({ jsonrpc: "2.0", id: message.id, error: { code: -32601, message: "unknown method" } });
  }
}

let buffer = "";
process.stdin.on("data", (chunk) => {
  buffer += chunk.toString("utf8");
  let newlineIndex;
  while ((newlineIndex = buffer.indexOf("\n")) !== -1) {
    const line = buffer.slice(0, newlineIndex).trim();
    buffer = buffer.slice(newlineIndex + 1);
    if (line.length === 0) {
      continue;
    }
    try {
      handle(JSON.parse(line));
    } catch {
      // ignore malformed input in this test fixture
    }
  }
});
