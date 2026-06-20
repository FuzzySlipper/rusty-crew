export interface DebugTuiApiClient {
  diagnostics(): Promise<DebugTuiDiagnosticsBundle>;
  sessions(query?: DebugTuiQuery): Promise<DebugTuiPage<DebugTuiSession>>;
  tools(query?: DebugTuiQuery): Promise<DebugTuiPage<DebugTuiToolCatalog>>;
  mcpSurfaces(query?: DebugTuiQuery): Promise<DebugTuiPage<DebugTuiMcpSurface>>;
  channelBindings(
    query?: DebugTuiQuery,
  ): Promise<DebugTuiPage<DebugTuiChannelBinding>>;
  observation(): Promise<DebugTuiObservation | null>;
  metrics(query?: DebugTuiQuery): Promise<DebugTuiPage<DebugTuiMetric>>;
  recentEvents(
    query?: DebugTuiQuery,
  ): Promise<DebugTuiPage<DebugTuiRecentEvent>>;
  directDebugContext(request: {
    sessionId: string;
  }): Promise<DebugTuiDirectDebugContext>;
}

export interface DebugTuiQuery {
  limit?: number;
  offset?: number;
  status?: string;
  profileId?: string;
  invalid?: boolean;
}

export interface DebugTuiPage<T> {
  items: T[];
  total: number;
  limit: number;
  offset: number;
  nextOffset?: number;
}

export interface DebugTuiDiagnosticsBundle {
  overview: DebugTuiDiagnosticsOverview;
  health: DebugTuiHealthProjection;
}

export interface DebugTuiDiagnosticsOverview {
  generatedAt: string;
  health: "ok" | "degraded" | "blocked";
  degraded: boolean;
  reasonCodes: readonly string[];
  summary: {
    activeSessions: number;
    idleSessions: number;
    archivedSessions: number;
    delegatedSessions: number;
    blockedDelegations: number;
    pendingQueueItems: number;
    expiredQueueItems: number;
  };
  queues?: {
    pending: number;
    expired: number;
    discarded?: number;
    oldestPendingAgeMs?: number;
  };
  persistence?: {
    schemaVersion?: number;
    databaseBytes?: number;
    pressure?: boolean;
    searchHealthy?: boolean;
  };
  issues: readonly {
    code: string;
    severity: "degraded" | "blocked";
    message: string;
  }[];
}

export interface DebugTuiHealthProjection {
  readiness: {
    ready: boolean;
  };
  metrics: DebugTuiMetric[];
}

export interface DebugTuiSession {
  sessionId: string;
  agentId: string;
  profileId: string;
  kind: string;
  status: string;
  toolCount: number;
  brainTurnCount: number;
  lastActiveAt: string;
  stale: boolean;
}

export interface DebugTuiToolCatalog {
  catalogId: string;
  registeredTools: number;
  selectedTools: number;
  validationErrors: number;
  validationWarnings: number;
  invalid: boolean;
}

export interface DebugTuiMcpSurface {
  bindingId: string;
  adapterId: string;
  agentId: string;
  sessionId?: string;
  profileId: string;
  status: string;
  transport: string;
  serverNames: string[];
  toolProfileKey: string;
  reconnectAttempts: number;
  collisionCount: number;
  discoveryIssueCount: number;
  optionalServerFailures: string[];
  lastError?: string;
}

export interface DebugTuiChannelBinding {
  bindingId: string;
  adapterId: string;
  agentId: string;
  sessionId?: string;
  profileId: string;
  provider: string;
  status: string;
  membershipStatus: string;
  presenceStatus: string;
  subscriptionStatus: string;
  stalePresence: boolean;
  droppedProjections: number;
  lastError?: string;
}

export interface DebugTuiObservation {
  enabled: boolean;
  writerAvailable: boolean;
  degraded: boolean;
  lastError?: string;
}

export interface DebugTuiMetric {
  name: string;
  value: number;
  labels?: Record<string, string>;
}

export interface DebugTuiRecentEvent {
  id: string | number;
  createdAt: string;
  source: string;
  eventType: string;
  severity?: string;
  summary: string;
}

export interface DebugTuiDirectDebugContext {
  generatedAt?: string;
  source: "direct_debug";
  session: {
    sessionId: string;
    agentId: string;
    profileId: string;
    kind: string;
    status: string;
    brainTurnCount: number;
    createdAt: string;
    lastActiveAt: string;
    toolCount: number;
    workdir?: string;
  };
  selectedTools: readonly { name: string; description: string }[];
  toolContext?: {
    summary: {
      selectedTools: number;
      deniedTools: number;
      missingTools: number;
    };
  };
  context: {
    rawPromptIncluded: boolean;
    rawPromptDeniedReason?: string;
    systemPrompt: {
      present?: boolean;
      chars: number;
      lines?: number;
      sha256?: string;
    };
    instructions: {
      present?: boolean;
      chars: number;
      lines?: number;
      sha256?: string;
    };
    sections: readonly string[];
    initialMessages?: {
      count: number;
      totalChars: number;
    };
    skills?: readonly {
      slug: string;
      title?: string;
      summary?: string;
      tags: readonly string[];
    }[];
  };
  pendingMessages: readonly unknown[];
  recentEvents?: readonly unknown[];
  controls: {
    directTurnInjection: string;
    reason?: string;
  };
}

export type DebugTuiTab =
  | "overview"
  | "sessions"
  | "channels"
  | "mcp"
  | "tools"
  | "queues"
  | "persistence"
  | "observation"
  | "events"
  | "context";

export type DebugTuiKey =
  | "ArrowUp"
  | "ArrowDown"
  | "ArrowLeft"
  | "ArrowRight"
  | "Tab"
  | "Shift+Tab"
  | "r"
  | "q";

export interface DebugTuiLoadOptions {
  activeSessionId?: string;
  includeDirectDebugContext?: boolean;
}

export interface DebugTuiSnapshot {
  diagnostics: DebugTuiDiagnosticsBundle;
  sessions: DebugTuiPage<DebugTuiSession>;
  tools: DebugTuiPage<DebugTuiToolCatalog>;
  mcp: DebugTuiPage<DebugTuiMcpSurface>;
  channels: DebugTuiPage<DebugTuiChannelBinding>;
  observation: DebugTuiObservation | null;
  metrics: DebugTuiPage<DebugTuiMetric>;
  events: DebugTuiPage<DebugTuiRecentEvent>;
  directDebugContext?: DebugTuiDirectDebugContext;
}

export interface DebugTuiState {
  generatedAt: string;
  activeTab: DebugTuiTab;
  selectedIndex: number;
  tabs: readonly DebugTuiTab[];
  status: "ok" | "degraded" | "blocked";
  readOnly: boolean;
  refreshRequested: boolean;
  quitRequested: boolean;
  snapshot: DebugTuiSnapshot;
}

export interface DebugTuiRenderOptions {
  width?: number;
  height?: number;
}

export async function loadDebugTuiState(
  client: DebugTuiApiClient,
  options: DebugTuiLoadOptions = {},
): Promise<DebugTuiState> {
  const [
    diagnostics,
    sessions,
    tools,
    mcp,
    channels,
    observation,
    metrics,
    events,
  ] = await Promise.all([
    client.diagnostics(),
    client.sessions({ limit: 100 }),
    client.tools({ limit: 100 }),
    client.mcpSurfaces({ limit: 100 }),
    client.channelBindings({ limit: 100 }),
    client.observation(),
    client.metrics({ limit: 100 }),
    client.recentEvents({ limit: 100 }),
  ]);
  const activeSessionId =
    options.activeSessionId ??
    sessions.items.find((item) => item.status === "active")?.sessionId;
  const directDebugContext =
    options.includeDirectDebugContext && activeSessionId
      ? await client.directDebugContext({ sessionId: activeSessionId })
      : undefined;

  return createDebugTuiState({
    diagnostics,
    sessions,
    tools,
    mcp,
    channels,
    observation,
    metrics,
    events,
    directDebugContext,
  });
}

export function createDebugTuiState(snapshot: DebugTuiSnapshot): DebugTuiState {
  return {
    generatedAt: snapshot.diagnostics.overview.generatedAt,
    activeTab: "overview",
    selectedIndex: 0,
    tabs: [
      "overview",
      "sessions",
      "channels",
      "mcp",
      "tools",
      "queues",
      "persistence",
      "observation",
      "events",
      ...(snapshot.directDebugContext ? (["context"] as const) : []),
    ],
    status: snapshot.diagnostics.overview.health,
    readOnly: true,
    refreshRequested: false,
    quitRequested: false,
    snapshot,
  };
}

export function reduceDebugTuiState(
  state: DebugTuiState,
  key: DebugTuiKey,
): DebugTuiState {
  switch (key) {
    case "ArrowUp":
      return {
        ...state,
        selectedIndex: Math.max(0, state.selectedIndex - 1),
      };
    case "ArrowDown":
      return {
        ...state,
        selectedIndex: Math.min(
          itemCountForTab(state) - 1,
          state.selectedIndex + 1,
        ),
      };
    case "ArrowLeft":
    case "Shift+Tab":
      return moveTab(state, -1);
    case "ArrowRight":
    case "Tab":
      return moveTab(state, 1);
    case "r":
      return { ...state, refreshRequested: true };
    case "q":
      return { ...state, quitRequested: true };
  }
}

export function renderDebugTui(
  state: DebugTuiState,
  options: DebugTuiRenderOptions = {},
): string {
  const width = Math.max(60, options.width ?? 100);
  const height = Math.max(12, options.height ?? 32);
  const lines = [
    headerLine(state, width),
    tabLine(state, width),
    divider(width),
    ...bodyLines(state),
    divider(width),
    footerLine(state, width),
  ];
  return lines
    .slice(0, height)
    .map((line) => truncate(line, width))
    .join("\n");
}

function bodyLines(state: DebugTuiState): string[] {
  switch (state.activeTab) {
    case "overview":
      return overviewLines(state);
    case "sessions":
      return tableLines(
        ["session", "agent", "profile", "status", "turns", "tools"],
        state.snapshot.sessions.items.map((session) => [
          session.sessionId,
          session.agentId,
          session.profileId,
          session.status,
          String(session.brainTurnCount),
          String(session.toolCount),
        ]),
        state.selectedIndex,
      );
    case "channels":
      return tableLines(
        ["binding", "agent", "profile", "status", "presence", "dropped"],
        state.snapshot.channels.items.map((binding) => [
          binding.bindingId,
          binding.agentId,
          binding.profileId,
          binding.status,
          binding.presenceStatus,
          String(binding.droppedProjections),
        ]),
        state.selectedIndex,
      );
    case "mcp":
      return tableLines(
        ["binding", "agent", "profile", "status", "servers", "collisions"],
        state.snapshot.mcp.items.map((surface) => [
          surface.bindingId,
          surface.agentId,
          surface.profileId,
          surface.status,
          surface.serverNames.join(","),
          String(surface.collisionCount),
        ]),
        state.selectedIndex,
      );
    case "tools":
      return tableLines(
        ["catalog", "registered", "selected", "errors", "warnings"],
        state.snapshot.tools.items.map((tool) => [
          tool.catalogId,
          String(tool.registeredTools),
          String(tool.selectedTools),
          String(tool.validationErrors),
          String(tool.validationWarnings),
        ]),
        state.selectedIndex,
      );
    case "queues":
      return keyValueLines({
        pending: state.snapshot.diagnostics.overview.queues?.pending ?? 0,
        expired: state.snapshot.diagnostics.overview.queues?.expired ?? 0,
        discarded:
          state.snapshot.diagnostics.overview.queues?.discarded ?? "unknown",
        oldestPendingAgeMs:
          state.snapshot.diagnostics.overview.queues?.oldestPendingAgeMs ??
          "unknown",
      });
    case "persistence":
      return keyValueLines({
        schemaVersion:
          state.snapshot.diagnostics.overview.persistence?.schemaVersion ??
          "unknown",
        databaseBytes:
          state.snapshot.diagnostics.overview.persistence?.databaseBytes ??
          "unknown",
        pressure:
          state.snapshot.diagnostics.overview.persistence?.pressure ?? false,
        searchHealthy:
          state.snapshot.diagnostics.overview.persistence?.searchHealthy ??
          "unknown",
      });
    case "observation":
      return keyValueLines({
        enabled: state.snapshot.observation?.enabled ?? false,
        writerAvailable: state.snapshot.observation?.writerAvailable ?? false,
        degraded: state.snapshot.observation?.degraded ?? "unknown",
        lastError: state.snapshot.observation?.lastError ?? "none",
      });
    case "events":
      return tableLines(
        ["created", "source", "type", "severity", "summary"],
        state.snapshot.events.items.map((event) => [
          event.createdAt,
          event.source,
          event.eventType,
          event.severity ?? "info",
          event.summary,
        ]),
        state.selectedIndex,
      );
    case "context":
      return contextLines(state.snapshot.directDebugContext);
  }
}

function overviewLines(state: DebugTuiState): string[] {
  const diagnostics = state.snapshot.diagnostics.overview;
  const health = state.snapshot.diagnostics.health;
  return [
    `health: ${badge(diagnostics.health)} ready=${health.readiness.ready} degraded=${diagnostics.degraded}`,
    `sessions: active=${diagnostics.summary.activeSessions} idle=${diagnostics.summary.idleSessions} archived=${diagnostics.summary.archivedSessions}`,
    `delegations: total=${diagnostics.summary.delegatedSessions} blocked=${diagnostics.summary.blockedDelegations}`,
    `queues: pending=${diagnostics.summary.pendingQueueItems} expired=${diagnostics.summary.expiredQueueItems}`,
    `channels: total=${state.snapshot.channels.total} degraded=${state.snapshot.channels.items.filter((item) => item.status === "degraded").length}`,
    `mcp: total=${state.snapshot.mcp.total} degraded=${state.snapshot.mcp.items.filter((item) => item.status === "degraded").length}`,
    `tools: catalogs=${state.snapshot.tools.total} invalid=${state.snapshot.tools.items.filter((item) => item.invalid).length}`,
    `observation: writer=${state.snapshot.observation?.writerAvailable ?? false}`,
    "",
    "issues:",
    ...diagnostics.issues
      .slice(0, 8)
      .map((issue) => `- ${issue.severity} ${issue.code}: ${issue.message}`),
  ];
}

function contextLines(
  context: DebugTuiDirectDebugContext | undefined,
): string[] {
  if (!context) {
    return ["No direct debug context loaded."];
  }
  return [
    `session: ${context.session.sessionId}`,
    `agent: ${context.session.agentId}`,
    `profile: ${context.session.profileId}`,
    `tools: selected=${context.toolContext?.summary.selectedTools ?? context.selectedTools.length} denied=${context.toolContext?.summary.deniedTools ?? 0} missing=${context.toolContext?.summary.missingTools ?? 0}`,
    `prompt: raw=${context.context.rawPromptIncluded} systemChars=${context.context.systemPrompt.chars} instructionChars=${context.context.instructions.chars}`,
    `sections: ${context.context.sections.join(", ") || "none"}`,
    `pending messages: ${context.pendingMessages.length}`,
    `direct turns: ${context.controls.directTurnInjection}`,
  ];
}

function tableLines(
  headers: readonly string[],
  rows: readonly (readonly string[])[],
  selectedIndex: number,
): string[] {
  return [
    headers.join(" | "),
    headers.map(() => "---").join(" | "),
    ...rows.map(
      (row, index) =>
        `${index === selectedIndex ? ">" : " "} ${row.join(" | ")}`,
    ),
  ];
}

function keyValueLines(values: Record<string, unknown>): string[] {
  return Object.entries(values).map(
    ([key, value]) => `${key}: ${String(value)}`,
  );
}

function moveTab(state: DebugTuiState, delta: number): DebugTuiState {
  const index = state.tabs.indexOf(state.activeTab);
  const next = (index + delta + state.tabs.length) % state.tabs.length;
  return {
    ...state,
    activeTab: state.tabs[next]!,
    selectedIndex: 0,
  };
}

function itemCountForTab(state: DebugTuiState): number {
  switch (state.activeTab) {
    case "sessions":
      return state.snapshot.sessions.items.length;
    case "channels":
      return state.snapshot.channels.items.length;
    case "mcp":
      return state.snapshot.mcp.items.length;
    case "tools":
      return state.snapshot.tools.items.length;
    case "events":
      return state.snapshot.events.items.length;
    case "overview":
    case "queues":
    case "persistence":
    case "observation":
    case "context":
      return 1;
  }
}

function headerLine(state: DebugTuiState, width: number): string {
  return truncate(
    `Rusty Crew Debug ${badge(state.status)} generated=${state.generatedAt} readOnly=${state.readOnly}`,
    width,
  );
}

function tabLine(state: DebugTuiState, width: number): string {
  return truncate(
    state.tabs
      .map((tab) => (tab === state.activeTab ? `[${tab}]` : ` ${tab} `))
      .join(" "),
    width,
  );
}

function footerLine(state: DebugTuiState, width: number): string {
  return truncate(
    `keys: arrows/tab navigate, r refresh, q quit | refresh=${state.refreshRequested} quit=${state.quitRequested}`,
    width,
  );
}

function divider(width: number): string {
  return "-".repeat(width);
}

function badge(status: string): string {
  switch (status) {
    case "ok":
      return "[OK]";
    case "degraded":
      return "[DEGRADED]";
    case "blocked":
      return "[BLOCKED]";
    default:
      return `[${status.toUpperCase()}]`;
  }
}

function truncate(value: string, width: number): string {
  return value.length > width ? `${value.slice(0, width - 3)}...` : value;
}
