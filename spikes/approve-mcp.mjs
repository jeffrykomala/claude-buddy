// Throwaway stdio MCP server for the Phase 2 spikes (docs/spikes.md): exposes
// one tool, `approve`, the way the real buddy permission server will, logs
// every call it receives as a JSON line, and answers allow (default) or deny.
//
//   SPIKE_LOG=<file>          where to append the received calls
//   SPIKE_DECISION=allow|deny what to answer (default allow)

import { appendFileSync } from "node:fs";
import { createInterface } from "node:readline";

const LOG = process.env.SPIKE_LOG;
const DECISION = process.env.SPIKE_DECISION ?? "allow";

function send(message) {
  process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", ...message })}\n`);
}

function handleCall(id, args) {
  if (LOG) {
    appendFileSync(LOG, `${JSON.stringify({ at: Date.now(), args })}\n`);
  }
  const verdict =
    DECISION === "deny"
      ? { behavior: "deny", message: "The user denied this action." }
      : { behavior: "allow", updatedInput: args.input };
  send({ id, result: { content: [{ type: "text", text: JSON.stringify(verdict) }] } });
}

const handlers = {
  initialize: (id, params) =>
    send({
      id,
      result: {
        protocolVersion: params.protocolVersion,
        capabilities: { tools: {} },
        serverInfo: { name: "buddy", version: "0.0.0" },
      },
    }),
  "tools/list": (id) =>
    send({
      id,
      result: {
        tools: [
          {
            name: "approve",
            description: "Decide whether a tool call may run.",
            inputSchema: {
              type: "object",
              properties: {
                tool_name: { type: "string" },
                input: { type: "object" },
              },
              required: ["tool_name", "input"],
            },
          },
        ],
      },
    }),
  "tools/call": (id, params) => handleCall(id, params.arguments ?? {}),
};

createInterface({ input: process.stdin }).on("line", (line) => {
  if (!line.trim()) {
    return;
  }
  const { id, method, params } = JSON.parse(line);
  const handler = handlers[method];
  if (handler && id !== undefined) {
    handler(id, params ?? {});
  } else if (id !== undefined) {
    send({ id, error: { code: -32601, message: `unsupported: ${method}` } });
  }
});
