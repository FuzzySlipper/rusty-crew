import { randomBytes } from "node:crypto";
import type { IncomingMessage } from "node:http";
import type { ProfileId, SessionId, SessionState } from "@rusty-crew/contracts";
import type { AdminRouteResult } from "../admin-diagnostics-api.js";
import { loadProfileConfig } from "../profile-loading.js";
import { failure, successRoute } from "../service-route-results.js";
import type { RoleplayRouteContext } from "../service-roleplay-routes.js";

interface MechanicAssociation {
  mechanicSessionId: string;
  mechanicProfileId: string;
  roleplaySessionId?: string;
  roleplayProfileId?: string;
  revision: number;
  createdAt: string;
  updatedAt: string;
}

interface MechanicDiagnostic {
  diagnosticId: string;
  revision: number;
}

export async function handleRoleplayMechanicSessionRequest(
  request: IncomingMessage,
  state: RoleplayRouteContext,
  url: URL,
): Promise<AdminRouteResult> {
  const requestId = routeRequestId(request);
  const method = (request.method ?? "GET").toUpperCase();
  const parts = url.pathname.split("/").filter(Boolean);
  const collection = parts[3];
  const id = parts[4] ? decodeURIComponent(parts[4]) : undefined;
  const action = parts[5];
  try {
    if (collection === "mechanic-sessions") {
      return await handleMechanicSessions(request, state, url, {
        requestId,
        method,
        id,
        action,
      });
    }
    return await handleMechanicDiagnostics(request, state, url, {
      requestId,
      method,
      id,
      action,
    });
  } catch (error) {
    return mechanicRouteError(requestId, error);
  }
}

async function handleMechanicSessions(
  request: IncomingMessage,
  state: RoleplayRouteContext,
  url: URL,
  route: {
    requestId: string;
    method: string;
    id?: string;
    action?: string;
  },
): Promise<AdminRouteResult> {
  if (route.id === undefined) {
    if (route.method === "GET") {
      const associations =
        (await state.bridge.listRoleplayMechanicSessionAssociations({
          mechanicProfileId: queryText(
            url,
            "mechanic_profile_id",
            "mechanicProfileId",
          ),
          roleplaySessionId: queryText(
            url,
            "roleplay_session_id",
            "roleplaySessionId",
          ),
          roleplayProfileId: queryText(
            url,
            "roleplay_profile_id",
            "roleplayProfileId",
          ),
          attached: queryBoolean(url, "attached"),
          page: queryPage(url),
        })) as MechanicAssociation[];
      const sessions = new Map<string, SessionState>(
        (await state.bridge.listSessions()).map((session) => [
          session.sessionId,
          session,
        ]),
      );
      return successRoute(route.requestId, {
        items: associations.map((association) => ({
          association,
          session: sessions.get(association.mechanicSessionId),
        })),
      });
    }
    if (route.method === "POST") {
      const body = bodyRecord(await readBody(request));
      return createdRoute(
        route.requestId,
        await createMechanicSession(state, body),
      );
    }
    return methodNotAllowed(
      route.requestId,
      "mechanic session collection supports GET and POST",
    );
  }

  if (route.action === undefined && route.method === "GET") {
    const association =
      (await state.bridge.getRoleplayMechanicSessionAssociation(route.id)) as
        | MechanicAssociation
        | undefined;
    if (association === undefined)
      return mechanicNotFound(route.requestId, route.id);
    const session = await state.serviceSessionById(route.id);
    return successRoute(route.requestId, { association, session });
  }
  if (route.action === "attach" && route.method === "POST") {
    const body = bodyRecord(await readBody(request));
    const association =
      await state.bridge.updateRoleplayMechanicSessionAttachment({
        mechanicSessionId: route.id,
        roleplaySessionId: optionalText(
          body.roleplaySessionId ?? body.roleplay_session_id,
        ),
        expectedRevision: positiveInteger(
          body.expectedRevision ?? body.expected_revision,
          "expectedRevision",
        ),
        now: state.now(),
      });
    return successRoute(route.requestId, { association });
  }
  if (route.action === "archive" && route.method === "POST") {
    const association = await requireAssociation(state, route.id);
    const session = await state.bridge.archiveSession(route.id as SessionId);
    return successRoute(route.requestId, { association, session });
  }
  if (route.action === "restore" && route.method === "POST") {
    const association = await requireAssociation(state, route.id);
    const existing = await state.serviceSessionById(route.id);
    const session = await state.bridge.ensureConfiguredSession(
      sessionCreateInput(existing),
    );
    return successRoute(route.requestId, { association, session });
  }
  return methodNotAllowed(
    route.requestId,
    "mechanic session supports GET and attach/archive/restore POST",
  );
}

async function handleMechanicDiagnostics(
  request: IncomingMessage,
  state: RoleplayRouteContext,
  url: URL,
  route: {
    requestId: string;
    method: string;
    id?: string;
    action?: string;
  },
): Promise<AdminRouteResult> {
  if (route.id === undefined) {
    if (route.method === "GET") {
      const items = await state.bridge.listRoleplayMechanicDiagnostics({
        mechanicSessionId: queryText(
          url,
          "mechanic_session_id",
          "mechanicSessionId",
        ),
        roleplaySessionId: queryText(
          url,
          "roleplay_session_id",
          "roleplaySessionId",
        ),
        roleplayProfileId: queryText(
          url,
          "roleplay_profile_id",
          "roleplayProfileId",
        ),
        outcome: queryText(url, "outcome"),
        proposalId: queryText(url, "proposal_id", "proposalId"),
        page: queryPage(url),
      });
      return successRoute(route.requestId, { items });
    }
    if (route.method === "POST") {
      const body = bodyRecord(await readBody(request));
      const diagnostic = await state.bridge.createRoleplayMechanicDiagnostic({
        diagnosticId:
          optionalText(body.diagnosticId ?? body.diagnostic_id) ??
          `mechanic-diagnostic:${randomBytes(16).toString("hex")}`,
        mechanicSessionId: requiredText(
          body.mechanicSessionId ?? body.mechanic_session_id,
          "mechanicSessionId",
        ),
        roleplaySessionId: requiredText(
          body.roleplaySessionId ?? body.roleplay_session_id,
          "roleplaySessionId",
        ),
        symptom: requiredText(body.symptom, "symptom"),
        hypothesis: requiredText(body.hypothesis, "hypothesis"),
        proposalIds: stringArray(
          body.proposalIds ?? body.proposal_ids,
          "proposalIds",
        ),
        appliedProposalIds: stringArray(
          body.appliedProposalIds ?? body.applied_proposal_ids,
          "appliedProposalIds",
        ),
        notes: optionalText(body.notes),
        now: state.now(),
      });
      return createdRoute(route.requestId, { diagnostic });
    }
    return methodNotAllowed(
      route.requestId,
      "mechanic diagnostic collection supports GET and POST",
    );
  }

  if (route.action === undefined && route.method === "GET") {
    const diagnostic = (await state.bridge.getRoleplayMechanicDiagnostic(
      route.id,
    )) as MechanicDiagnostic | undefined;
    return diagnostic === undefined
      ? failure(404, route.requestId, {
          code: "not_found",
          reason_code: "roleplay_mechanic_diagnostic_not_found",
          message: `roleplay mechanic diagnostic ${route.id} was not found`,
          retryable: false,
        })
      : successRoute(route.requestId, { diagnostic });
  }
  if (route.action === "outcome" && route.method === "POST") {
    const body = bodyRecord(await readBody(request));
    const diagnostic =
      await state.bridge.updateRoleplayMechanicDiagnosticOutcome({
        diagnosticId: route.id,
        outcome: requiredText(body.outcome, "outcome"),
        notes: optionalText(body.notes),
        expectedRevision: positiveInteger(
          body.expectedRevision ?? body.expected_revision,
          "expectedRevision",
        ),
        now: state.now(),
      });
    return successRoute(route.requestId, { diagnostic });
  }
  return methodNotAllowed(
    route.requestId,
    "mechanic diagnostic supports GET and outcome POST",
  );
}

async function createMechanicSession(
  state: RoleplayRouteContext,
  body: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  const profileId = requiredText(
    body.profileId ?? body.profile_id,
    "profileId",
  ) as ProfileId;
  const profile = await loadProfileConfig(
    state.runtimeConfig.profilesDir,
    profileId,
  );
  if (profile.roleplayMechanic === undefined) {
    throw new Error(
      `profile ${profileId} is not configured as a roleplay mechanic`,
    );
  }
  const registry = await state.bridge.getProfileRegistryRecord(profileId);
  if (registry === undefined)
    throw new Error(`profile ${profileId} is not registered`);
  const roleplaySessionId = optionalText(
    body.roleplaySessionId ?? body.roleplay_session_id,
  );
  const now = state.now();
  const agentId = registry.agentId ?? profileId;
  const sessionId = `${agentId}-mechanic-${now.replace(/[^0-9A-Za-z]/g, "").slice(0, 17)}-${randomBytes(3).toString("hex")}`;
  const session = await state.bridge.createSession({
    sessionId,
    agentId,
    profileId,
    kind: "full",
    resourceLimits: {},
    toolProfile: { tools: [] },
  });
  try {
    const association =
      await state.bridge.createRoleplayMechanicSessionAssociation({
        mechanicSessionId: sessionId,
        roleplaySessionId,
        now,
      });
    return { association, session };
  } catch (error) {
    await state.bridge
      .archiveSession(sessionId as SessionId)
      .catch(() => undefined);
    throw error;
  }
}

async function requireAssociation(
  state: RoleplayRouteContext,
  sessionId: string,
): Promise<MechanicAssociation> {
  const association = (await state.bridge.getRoleplayMechanicSessionAssociation(
    sessionId,
  )) as MechanicAssociation | undefined;
  if (association === undefined)
    throw new Error(`mechanic session ${sessionId} was not found`);
  return association;
}

function sessionCreateInput(session: SessionState) {
  return {
    sessionId: session.sessionId,
    agentId: session.agentId,
    profileId: session.profileId,
    kind: session.kind,
    resourceLimits: session.resourceLimits,
    toolProfile: session.toolProfile,
    historyWindow: session.historyWindow,
  };
}

async function readBody(request: IncomingMessage): Promise<unknown> {
  const chunks: Buffer[] = [];
  let length = 0;
  for await (const chunk of request) {
    const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    length += buffer.length;
    if (length > 1_048_576) throw new Error("request body exceeds 1 MiB");
    chunks.push(buffer);
  }
  if (chunks.length === 0) return {};
  return JSON.parse(Buffer.concat(chunks).toString("utf8")) as unknown;
}

function bodyRecord(value: unknown): Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("request body must be a JSON object");
  }
  return value as Record<string, unknown>;
}

function requiredText(value: unknown, name: string): string {
  const text = optionalText(value);
  if (text === undefined) throw new Error(`${name} is required`);
  return text;
}

function optionalText(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() !== ""
    ? value.trim()
    : undefined;
}

function positiveInteger(value: unknown, name: string): number {
  if (!Number.isSafeInteger(value) || (value as number) < 1) {
    throw new Error(`${name} must be a positive integer`);
  }
  return value as number;
}

function stringArray(value: unknown, name: string): string[] {
  if (value === undefined) return [];
  if (
    !Array.isArray(value) ||
    value.some((item) => optionalText(item) === undefined)
  ) {
    throw new Error(`${name} must be an array of non-empty strings`);
  }
  return value.map((item) => (item as string).trim());
}

function queryText(url: URL, ...names: string[]): string | undefined {
  for (const name of names) {
    const value = optionalText(url.searchParams.get(name));
    if (value !== undefined) return value;
  }
  return undefined;
}

function queryBoolean(url: URL, name: string): boolean | undefined {
  const value = url.searchParams.get(name);
  if (value === null) return undefined;
  if (value === "true") return true;
  if (value === "false") return false;
  throw new Error(`${name} must be true or false`);
}

function queryPage(url: URL): { limit?: number; offset?: number } {
  const parse = (name: string) => {
    const raw = url.searchParams.get(name);
    if (raw === null) return undefined;
    const value = Number(raw);
    if (!Number.isSafeInteger(value) || value < 0)
      throw new Error(`${name} must be an integer`);
    return value;
  };
  return { limit: parse("limit"), offset: parse("offset") };
}

function routeRequestId(request: IncomingMessage): string {
  const provided = request.headers["x-request-id"];
  return (
    (Array.isArray(provided) ? provided[0] : provided) ??
    randomBytes(12).toString("hex")
  );
}

function createdRoute(requestId: string, data: unknown): AdminRouteResult {
  const result = successRoute(requestId, data);
  return { ...result, status: 201 };
}

function mechanicNotFound(
  requestId: string,
  sessionId: string,
): AdminRouteResult {
  return failure(404, requestId, {
    code: "not_found",
    reason_code: "roleplay_mechanic_session_not_found",
    message: `roleplay mechanic session ${sessionId} was not found`,
    retryable: false,
  });
}

function methodNotAllowed(
  requestId: string,
  message: string,
): AdminRouteResult {
  return failure(405, requestId, {
    code: "method_not_allowed",
    reason_code: "roleplay_mechanic_method_not_allowed",
    message,
    retryable: false,
  });
}

function mechanicRouteError(
  requestId: string,
  error: unknown,
): AdminRouteResult {
  const message = error instanceof Error ? error.message : String(error);
  const conflict = message.includes("revision mismatch");
  return failure(conflict ? 409 : 400, requestId, {
    code: conflict ? "conflict" : "invalid_input",
    reason_code: conflict
      ? "roleplay_mechanic_revision_conflict"
      : "roleplay_mechanic_request_failed",
    message,
    retryable: false,
  });
}
