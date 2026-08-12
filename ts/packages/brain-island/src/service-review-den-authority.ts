import type { McpBindingRecord } from "@rusty-crew/contracts";
import type { RustyCrewReviewDenAuthorityConfig } from "./service-config.js";
import {
  buildServiceMcpEndpointConfig,
  listConfiguredMcpTools,
  type ServiceMcpEndpointConfig,
  type ServiceMcpEndpointIdentity,
  type ServiceMcpServerEndpointConfig,
} from "./service-mcp-tools.js";

export const REVIEW_DEN_REQUIRED_TOOLS = [
  "finalize_review",
  "get_task",
  "get_github_check_gate",
  "list_projects",
  "list_review_pipeline",
  "list_review_rounds",
  "request_review",
  "update_task",
  "watch_github_checks",
] as const;

export type ReviewDenToolName = (typeof REVIEW_DEN_REQUIRED_TOOLS)[number];

export type ReviewDenAuthority =
  | {
      kind: "submitter_binding";
      binding: McpBindingRecord;
      bindingId: string;
      auditIdentity: string;
    }
  | {
      kind: "service";
      config: RustyCrewReviewDenAuthorityConfig;
      binding: ServiceMcpEndpointIdentity;
      bindingId: string;
      auditIdentity: string;
    };

export interface ReviewDenAuthorityDiagnostics {
  authorityId?: string;
  auditIdentity?: string;
  serverName: "den";
  status: "ready" | "unconfigured" | "unavailable" | "missing_tools";
  requiredTools: readonly ReviewDenToolName[];
  missingTools: string[];
  checkedAt: string;
  message: string;
}

export function serviceReviewDenAuthority(
  config: RustyCrewReviewDenAuthorityConfig | undefined,
): Extract<ReviewDenAuthority, { kind: "service" }> | undefined {
  if (config === undefined) return undefined;
  return {
    kind: "service",
    config,
    bindingId: config.authorityId,
    auditIdentity: config.auditIdentity,
    binding: {
      bindingId: config.authorityId,
      endpointRef: config.endpointRef,
      toolProfileKey: config.toolProfileKey,
    },
  };
}

export async function validateServiceReviewDenAuthority(input: {
  authority: RustyCrewReviewDenAuthorityConfig | undefined;
  mcpConfig?: ServiceMcpEndpointConfig;
  mcpServers?: readonly ServiceMcpServerEndpointConfig[];
  now(): string;
  listTools?: () => Promise<unknown[]>;
}): Promise<ReviewDenAuthorityDiagnostics> {
  const checkedAt = input.now();
  const authority = serviceReviewDenAuthority(input.authority);
  if (authority === undefined) {
    return {
      serverName: "den",
      status: "unconfigured",
      requiredTools: REVIEW_DEN_REQUIRED_TOOLS,
      missingTools: [...REVIEW_DEN_REQUIRED_TOOLS],
      checkedAt,
      message: "Dedicated service review Den authority is not configured.",
    };
  }
  try {
    const tools = await (input.listTools?.() ??
      listConfiguredMcpTools({
        binding: authority.binding,
        config: buildServiceMcpEndpointConfig({
          mcpConfig: input.mcpConfig,
          mcpServers: input.mcpServers,
        }),
        ...(authority.config.bearerToken === undefined
          ? {}
          : { bearerToken: authority.config.bearerToken }),
        clientName: authority.auditIdentity,
      }));
    const names = new Set(
      tools.flatMap((tool) => {
        if (typeof tool !== "object" || tool === null || Array.isArray(tool)) {
          return [];
        }
        const name = (tool as Record<string, unknown>).name;
        return typeof name === "string" ? [name] : [];
      }),
    );
    const missingTools = REVIEW_DEN_REQUIRED_TOOLS.filter(
      (name) => !names.has(name),
    );
    return {
      authorityId: authority.bindingId,
      auditIdentity: authority.auditIdentity,
      serverName: "den",
      status: missingTools.length === 0 ? "ready" : "missing_tools",
      requiredTools: REVIEW_DEN_REQUIRED_TOOLS,
      missingTools,
      checkedAt,
      message:
        missingTools.length === 0
          ? "Dedicated service review Den authority is ready."
          : `Dedicated service review Den authority is missing required tools: ${missingTools.join(", ")}.`,
    };
  } catch (error) {
    return {
      authorityId: authority.bindingId,
      auditIdentity: authority.auditIdentity,
      serverName: "den",
      status: "unavailable",
      requiredTools: REVIEW_DEN_REQUIRED_TOOLS,
      missingTools: [...REVIEW_DEN_REQUIRED_TOOLS],
      checkedAt,
      message: error instanceof Error ? error.message : String(error),
    };
  }
}

export function isReviewDenToolName(name: string): name is ReviewDenToolName {
  return (REVIEW_DEN_REQUIRED_TOOLS as readonly string[]).includes(name);
}
