import type {
  AgentDirectoryEntry,
  AgentMessageDeliveryReceipt,
  AgentRouteResolution,
} from "@rusty-crew/contracts";

export function ambiguousBareRoute(
  address: string,
  routes: readonly AgentRouteResolution[],
): AgentRouteResolution | undefined {
  if (address.startsWith("@")) return undefined;
  return routes.find((resolution) => resolution.route?.routeKey === address);
}

export function ambiguousBareRouteMessage(
  address: string,
  routes: readonly AgentRouteResolution[],
): string | undefined {
  const collision = ambiguousBareRoute(address, routes);
  return collision === undefined
    ? undefined
    : `recipient ${address} is ambiguous because ${collision.address} is a curated route; use ${collision.address}, or use the operator coordination API for an explicit raw-agent diagnostic delivery`;
}

export function formatModelAgentDirectory(
  routes: readonly AgentRouteResolution[],
  agents: readonly AgentDirectoryEntry[],
): string {
  const routeKeys = new Set(
    routes.flatMap(({ route }) => (route == null ? [] : [route.routeKey])),
  );
  const routedAgentIds = new Set(
    routes.flatMap(({ resolvedTarget }) =>
      resolvedTarget == null ? [] : [resolvedTarget.agentId],
    ),
  );
  const rawAgents = agents.filter(
    (agent) =>
      !routeKeys.has(agent.agentId) && !routedAgentIds.has(agent.agentId),
  );
  const omittedCount = agents.length - rawAgents.length;

  return [
    "Switchboard routes (preferred model-callable addresses):",
    ...(routes.length === 0
      ? ["- none configured"]
      : routes.map((resolution) => {
          const target = resolution.resolvedTarget;
          const status = resolution.routable
            ? "routable"
            : `unavailable (${resolution.reasonCode ?? "unknown_reason"})`;
          return `- ${resolution.address}: ${resolution.route?.label ?? "unknown route"}; target=${target?.agentId ?? "unresolved"}; session=${target?.sessionId ?? "unresolved"}; runtime=${target?.runtimeKind ?? resolution.route?.requiredRuntimeKind ?? "unresolved"}; status=${status}`;
        })),
    "",
    "Unrouted raw agents (model-callable diagnostics):",
    ...(rawAgents.length === 0
      ? ["- no unrouted raw agents registered"]
      : rawAgents.map((agent) => {
          const status = agent.routable
            ? "routable"
            : `unavailable (${agent.routabilityReasonCode ?? "unknown_reason"})`;
          const task = agent.taskRef?.projectId
            ? `; project=${agent.taskRef.projectId}`
            : "";
          return `- ${agent.displayLabel}: recipient=${agent.agentId}; profile=${agent.profileId}; session=${agent.sessionId}; runtime=${agent.runtimeKind}; status=${status}${task}`;
        })),
    ...(omittedCount === 0
      ? []
      : [
          `- ${omittedCount} raw diagnostic entr${omittedCount === 1 ? "y" : "ies"} omitted because curated routes already represent them or reserve their keys`,
        ]),
  ].join("\n");
}

export function formatDeliveryTarget(
  receipt: AgentMessageDeliveryReceipt,
): string {
  const activation = receipt.activation?.type ?? "none";
  const runtime =
    receipt.request.routing?.resolvedTarget.runtimeKind ??
    (activation.startsWith("external_turn")
      ? "codex_app_server"
      : "direct_brain");
  const addressKind =
    receipt.request.routing === undefined || receipt.request.routing === null
      ? "raw_agent"
      : `curated_route:${receipt.request.routing.address}`;
  return `address=${receipt.request.requestedAddress}; addressKind=${addressKind}; agent=${receipt.request.toAgentId}; session=${receipt.request.toSessionId ?? "none"}; runtime=${runtime}; activation=${activation}`;
}
