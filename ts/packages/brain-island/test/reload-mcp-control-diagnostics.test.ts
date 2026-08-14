import assert from "node:assert/strict";
import test from "node:test";

import { createReloadMcpControlExecutor } from "../src/reload-mcp-control.js";

const command = {
  name: "reload_mcp" as const,
  requestId: "reload-request",
  idempotencyKey: "reload-request",
  actor: { operatorId: "test-operator" },
  target: { sessionId: "session-1" },
};

test("reload MCP distinguishes native fallback unavailability from a loaded planner failure", async () => {
  const unavailable = createReloadMcpControlExecutor(
    failureOnlyOptions(async () => {
      throw new Error(
        "native bridge operation plan_reload_mcp_control is unavailable",
      );
    }),
  );
  const failed = createReloadMcpControlExecutor(
    failureOnlyOptions(async () => {
      throw new Error("native planner transport disconnected");
    }),
  );

  assert.equal(
    (await unavailable(command as never)).reasonCode,
    "native_reload_mcp_planner_unavailable",
  );
  assert.equal(
    (await failed(command as never)).reasonCode,
    "native_reload_mcp_planner_failed",
  );
});

test("reload MCP preserves Rust denial reason codes such as a missing exact binding", async () => {
  const executor = createReloadMcpControlExecutor(
    failureOnlyOptions(async () => ({
      accepted: false,
      commandKind: "reload_mcp",
      target: { sessionId: "session-1" },
      operatorReason: "test",
      reasonCode: "mcp_binding_not_found",
      preconditions: [],
      actions: [],
      denial: {
        reasonCode: "mcp_binding_not_found",
        summary: "No exact-session MCP binding is materialized.",
      },
    })),
  );

  const result = await executor(command as never);
  assert.equal(result.reasonCode, "mcp_binding_not_found");
  assert.equal(result.summary, "No exact-session MCP binding is materialized.");
});

function failureOnlyOptions(
  planReloadMcpControl: Parameters<
    typeof createReloadMcpControlExecutor
  >[0]["planReloadMcpControl"],
): Parameters<typeof createReloadMcpControlExecutor>[0] {
  return {
    resolveBinding: () => undefined,
    planReloadMcpControl,
    manager: {} as never,
    discoveryClient: {} as never,
    metadataPolicyValidator: {} as never,
    catalogId: () => "unused",
  };
}
