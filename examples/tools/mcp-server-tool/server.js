#!/usr/bin/env node
// Minimal stdio-framed JSON-RPC server used as a Tuff MCP tool example.

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
  const body = JSON.stringify(message);
  process.stdout.write(`Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`);
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

let buffer = Buffer.alloc(0);

process.stdin.on("data", (chunk) => {
  buffer = Buffer.concat([buffer, chunk]);

  while (true) {
    const headerEnd = buffer.indexOf("\r\n\r\n");
    if (headerEnd === -1) {
      return;
    }

    const header = buffer.slice(0, headerEnd).toString("utf8");
    const lengthLine = header
      .split("\r\n")
      .find((line) => line.toLowerCase().startsWith("content-length:"));
    if (!lengthLine) {
      error(null, -32600, "Missing Content-Length header");
      buffer = Buffer.alloc(0);
      return;
    }

    const length = Number.parseInt(lengthLine.split(":")[1].trim(), 10);
    const bodyStart = headerEnd + 4;
    const bodyEnd = bodyStart + length;
    if (buffer.length < bodyEnd) {
      return;
    }

    const body = buffer.slice(bodyStart, bodyEnd).toString("utf8");
    buffer = buffer.slice(bodyEnd);

    try {
      handle(JSON.parse(body));
    } catch {
      error(null, -32700, "Invalid JSON");
    }
  }
});
