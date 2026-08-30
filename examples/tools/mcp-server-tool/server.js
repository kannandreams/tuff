#!/usr/bin/env node
// Minimal stdio MCP server used as a Tuff MCP tool example.
//
// Framing is newline-delimited JSON-RPC: one JSON object per line on
// stdin/stdout, exactly what the real MCP stdio transport spec requires
// (and what `tuff mcp doctor` speaks) -- not Content-Length headers.

const guidance = {
  tests: "Run focused tests first, then the broader suite before handing off.",
  diff: "Review generated files, lockfiles, and agent capability baselines.",
  release: "Check version metadata, docs, changelog, and installation notes.",
};

function respond(id, result) {
  send({ jsonrpc: "2.0", id, result });
}

function error(id, code, message) {
  send({ jsonrpc: "2.0", id, error: { code, message } });
}

function send(message) {
  process.stdout.write(JSON.stringify(message) + "\n");
}

function toolList() {
  return {
    tools: [
      {
        name: "repo_guidance",
        description: "Return short repository guidance for a known topic.",
        inputSchema: {
          type: "object",
          required: ["topic"],
          properties: {
            topic: {
              type: "string",
              enum: Object.keys(guidance),
            },
          },
        },
      },
    ],
  };
}

function callTool(params) {
  const topic = params?.arguments?.topic || "tests";
  return {
    content: [
      {
        type: "text",
        text: guidance[topic] || `Unknown topic: ${topic}`,
      },
    ],
  };
}

function handle(message) {
  if (!message || typeof message !== "object") {
    error(null, -32600, "Invalid request");
    return;
  }

  if (message.id === undefined) {
    // A notification (e.g. notifications/initialized) -- nothing to reply to.
    return;
  }

  if (message.method === "initialize") {
    respond(message.id, {
      protocolVersion: "2024-11-05",
      capabilities: { tools: {} },
      serverInfo: { name: "tuff-example-mcp-server", version: "0.1.0" },
    });
  } else if (message.method === "tools/list") {
    respond(message.id, toolList());
  } else if (message.method === "tools/call") {
    respond(message.id, callTool(message.params));
  } else {
    error(message.id, -32601, `Unknown method: ${message.method}`);
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
      error(null, -32700, "Invalid JSON");
    }
  }
});
