import type {
  AdapterId,
  McpBindingRecord,
  McpSurfaceDiagnostics,
  McpSurfaceIdentity,
  McpSurfaceStatus,
  McpTransportKind,
  PlatformAdapterRegistration,
} from "@rusty-crew/contracts";

export function createMcpAdapterRegistration(
  adapterId: AdapterId,
): PlatformAdapterRegistration {
  return { adapterId, kind: "mcp", displayName: "MCP" };
}

export * from "./mcp-discovery.js";

export interface McpTransportOpenRequest {
  binding: McpBindingRecord;
  endpointRef: string;
  serverNames: readonly string[];
}

export interface McpTransportClient {
  readonly kind: McpTransportKind | string;
  readonly name: string;
  connect(request: McpTransportOpenRequest): Promise<void> | void;
  disconnect(): Promise<void> | void;
  ping?(): Promise<void> | void;
}

export interface McpTransportFactory {
  readonly kind: McpTransportKind | string;
  create(binding: McpBindingRecord): McpTransportClient;
}

export interface McpBackoffPolicy {
  maxAttempts: number;
  backoffMs: readonly number[];
}

export interface McpSurfaceManagerOptions {
  transports: readonly McpTransportFactory[];
  backoff?: McpBackoffPolicy;
  now?: () => string;
}

export interface McpConnectResult {
  bindingId: string;
  status: McpSurfaceStatus;
  transport: McpTransportKind | string;
  attemptCount: number;
  optional: boolean;
  degradedReason?: string;
}

interface McpSurfaceState {
  binding: McpBindingRecord;
  client?: McpTransportClient;
  status: McpSurfaceStatus;
  connectedAt?: string;
  lastCheckedAt?: string;
  lastError?: string;
  reconnectAttempts: number;
  optional: boolean;
}

export class McpSurfaceManager {
  readonly #transports: Map<McpTransportKind | string, McpTransportFactory>;
  readonly #backoff: McpBackoffPolicy;
  readonly #now: () => string;
  readonly #surfaces = new Map<string, McpSurfaceState>();

  constructor(options: McpSurfaceManagerOptions) {
    this.#transports = new Map(
      options.transports.map((factory) => [factory.kind, factory]),
    );
    this.#backoff = options.backoff ?? {
      maxAttempts: 3,
      backoffMs: [250, 1_000, 5_000],
    };
    this.#now = options.now ?? (() => new Date().toISOString());
  }

  async connect(binding: McpBindingRecord): Promise<McpConnectResult> {
    const optional = isOptionalMcpBinding(binding);
    const factory = this.#transports.get(binding.transport);
    const state = this.#stateFor(binding, optional);

    if (factory === undefined) {
      return this.#markDegraded(
        state,
        `no MCP transport factory registered for ${binding.transport}`,
        0,
      );
    }

    await state.client?.disconnect();
    state.status = "connecting";
    state.lastCheckedAt = this.#now();
    state.binding = binding;

    const maxAttempts = Math.max(1, this.#backoff.maxAttempts);
    for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
      const client = factory.create(binding);
      try {
        await client.connect({
          binding,
          endpointRef: binding.endpointRef,
          serverNames: binding.serverNames,
        });
        state.client = client;
        state.status = "active";
        state.connectedAt = this.#now();
        state.lastCheckedAt = state.connectedAt;
        state.lastError = undefined;
        return {
          bindingId: binding.bindingId,
          status: state.status,
          transport: binding.transport,
          attemptCount: attempt,
          optional,
        };
      } catch (error) {
        state.reconnectAttempts += 1;
        state.lastError = errorMessage(error);
        state.lastCheckedAt = this.#now();
      }
    }

    return this.#markDegraded(
      state,
      state.lastError ?? "MCP surface connection failed",
      maxAttempts,
    );
  }

  async reconnect(bindingId: string): Promise<McpConnectResult | undefined> {
    const state = this.#surfaces.get(bindingId);
    if (state === undefined || state.status === "archived") return undefined;
    return this.connect(state.binding);
  }

  async disconnect(
    bindingId: string,
  ): Promise<McpSurfaceDiagnostics | undefined> {
    const state = this.#surfaces.get(bindingId);
    if (state === undefined) return undefined;
    await state.client?.disconnect();
    state.client = undefined;
    state.status = "disconnected";
    state.lastCheckedAt = this.#now();
    return this.diagnostics(bindingId);
  }

  async archive(bindingId: string): Promise<McpSurfaceDiagnostics | undefined> {
    const state = this.#surfaces.get(bindingId);
    if (state === undefined) return undefined;
    await state.client?.disconnect();
    state.client = undefined;
    state.status = "archived";
    state.lastCheckedAt = this.#now();
    return this.diagnostics(bindingId);
  }

  async reload(binding: McpBindingRecord): Promise<McpConnectResult> {
    await this.disconnect(binding.bindingId);
    return this.connect(binding);
  }

  async shutdown(): Promise<McpSurfaceDiagnostics[]> {
    const diagnostics: McpSurfaceDiagnostics[] = [];
    for (const bindingId of [...this.#surfaces.keys()].sort()) {
      const archived = await this.archive(bindingId);
      if (archived) diagnostics.push(archived);
    }
    return diagnostics;
  }

  identity(bindingId: string): McpSurfaceIdentity | undefined {
    const binding = this.#surfaces.get(bindingId)?.binding;
    if (binding === undefined) return undefined;
    return {
      bindingId: binding.bindingId,
      adapterId: binding.adapterId,
      agentId: binding.agentId,
      instanceId: binding.instanceId,
      sessionId: binding.sessionId,
      profileId: binding.profileId,
      serverNames: [...binding.serverNames],
      toolProfileKey: binding.toolProfileKey,
    };
  }

  diagnostics(): McpSurfaceDiagnostics[];
  diagnostics(bindingId: string): McpSurfaceDiagnostics | undefined;
  diagnostics(
    bindingId?: string,
  ): McpSurfaceDiagnostics | McpSurfaceDiagnostics[] | undefined {
    if (bindingId !== undefined) {
      const state = this.#surfaces.get(bindingId);
      return state ? diagnosticsFromState(state) : undefined;
    }
    return [...this.#surfaces.values()]
      .sort((left, right) =>
        left.binding.bindingId.localeCompare(right.binding.bindingId),
      )
      .map(diagnosticsFromState);
  }

  #stateFor(binding: McpBindingRecord, optional: boolean): McpSurfaceState {
    const existing = this.#surfaces.get(binding.bindingId);
    if (existing) {
      existing.binding = binding;
      existing.optional = optional;
      return existing;
    }
    const state: McpSurfaceState = {
      binding,
      status: "disconnected",
      reconnectAttempts: 0,
      optional,
    };
    this.#surfaces.set(binding.bindingId, state);
    return state;
  }

  #markDegraded(
    state: McpSurfaceState,
    reason: string,
    attemptCount: number,
  ): McpConnectResult {
    state.status = "degraded";
    state.lastError = reason;
    state.lastCheckedAt = this.#now();
    return {
      bindingId: state.binding.bindingId,
      status: state.status,
      transport: state.binding.transport,
      attemptCount,
      optional: state.optional,
      degradedReason: reason,
    };
  }
}

export function createSimulatedMcpTransportFactory(
  kind: McpTransportKind,
  options: {
    name?: string;
    failConnects?: number;
  } = {},
): McpTransportFactory & {
  readonly opened: McpTransportOpenRequest[];
  readonly disconnected: string[];
} {
  const opened: McpTransportOpenRequest[] = [];
  const disconnected: string[] = [];
  let remainingFailures = options.failConnects ?? 0;
  return {
    kind,
    opened,
    disconnected,
    create(binding) {
      return {
        kind,
        name: options.name ?? `${kind}:${binding.bindingId}`,
        connect(request) {
          if (remainingFailures > 0) {
            remainingFailures -= 1;
            throw new Error(`simulated ${kind} connect failure`);
          }
          opened.push(request);
        },
        disconnect() {
          disconnected.push(binding.bindingId);
        },
      };
    },
  };
}

function diagnosticsFromState(state: McpSurfaceState): McpSurfaceDiagnostics {
  return {
    bindingId: state.binding.bindingId,
    status: state.status,
    transport: state.binding.transport,
    serverNames: [...state.binding.serverNames],
    endpointRef: state.binding.endpointRef,
    toolProfileKey: state.binding.toolProfileKey,
    connectedAt: state.connectedAt,
    lastCheckedAt: state.lastCheckedAt,
    lastError: state.lastError,
    reconnectAttempts: state.reconnectAttempts,
    optional: state.optional,
  };
}

function isOptionalMcpBinding(binding: McpBindingRecord): boolean {
  return binding.diagnostics.notes?.includes("optional") ?? false;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
