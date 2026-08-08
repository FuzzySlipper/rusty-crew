import type {
  BrainEvent,
  ProfileId,
  SessionId,
  SessionState,
} from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import type {
  ManualContextCompactionInput,
  ManualContextCompactionResult,
} from "./rusty-view-chat-api.js";
import {
  appendCoreEventsToChatLog,
  observeWakeEvents,
  type ServiceWakeDispatchContext,
} from "./service-wake-dispatch.js";
import { buildProfileRoleAssembly } from "./profile-role-assembly.js";
import { effectiveToolSelectionForResourceLimits } from "./tool-profile-selection.js";

export function manualCompactionEffectiveFingerprint(input: {
  intentKey?: string | null;
  sourceProjectionFingerprint?: string | null;
}): string {
  return (
    input.sourceProjectionFingerprint ?? `manual-${input.intentKey ?? "manual"}`
  );
}

export function manualCompactionArtifactEffectiveFingerprint(artifact: {
  intent_key?: string | null;
  source_projection_fingerprint?: string | null;
}): string | undefined {
  if (artifact.source_projection_fingerprint)
    return artifact.source_projection_fingerprint;
  if (artifact.intent_key) return `manual-${artifact.intent_key}`;
  return undefined;
}

export function isManualCompactionDuplicate(
  artifact: {
    intent_key?: string | null;
    source_projection_fingerprint?: string | null;
    session_id: string;
  },
  input: {
    intentKey?: string | null;
    sessionId: string;
    sourceProjectionFingerprint?: string | null;
  },
  effectiveFingerprint: string,
): boolean {
  return (
    artifact.intent_key === input.intentKey &&
    artifact.session_id === input.sessionId &&
    (artifact.source_projection_fingerprint ??
      `manual-${artifact.intent_key}`) === effectiveFingerprint
  );
}

/**
 * Dependencies for the production manual-compaction operation.
 *
 * `bridge` is the durable artifact authority (list/read + synthetic native
 * fallback); `dispatch` is the wake dispatch context used to build and observe
 * the real Rust brain wake. service-app wires this with `state.bridge` and
 * `wakeDispatchContext(state)`; tests inject a double with authoritative
 * list/save/read behavior so the public boundary regression exercises the real
 * production path (R6624-11/12).
 */
export interface ManualCompactionDeps {
  bridge: Pick<
    NativeBridgeModule,
    "listContextCompactionArtifacts" | "manualContextCompaction"
  >;
  dispatch: ServiceWakeDispatchContext;
}

export async function runManualContextCompaction(
  deps: ManualCompactionDeps,
  input: ManualContextCompactionInput,
): Promise<ManualContextCompactionResult> {
  const effectiveFingerprint = manualCompactionEffectiveFingerprint(input);
  const existing = await deps.bridge.listContextCompactionArtifacts({
    session_id: input.session.sessionId,
    branch_id: undefined,
    strategy_id: undefined,
    enters_future_context: undefined,
    latest_only: false,
    limit: 1000,
    offset: 0,
  });
  const duplicate = existing.find((artifact) =>
    isManualCompactionDuplicate(
      artifact,
      {
        intentKey: input.intentKey,
        sessionId: input.session.sessionId,
        sourceProjectionFingerprint: input.sourceProjectionFingerprint,
      },
      effectiveFingerprint,
    ),
  );
  if (duplicate) {
    const revision = duplicate.strategy_revision
      ? Number.parseInt(duplicate.strategy_revision, 10)
      : 0;
    if (
      input.expectRevision !== null &&
      input.expectRevision !== undefined &&
      Number.isFinite(input.expectRevision) &&
      revision !== input.expectRevision
    ) {
      throw new Error(
        `revision_conflict: expected ${input.expectRevision} but found ${revision}`,
      );
    }
    return {
      ok: true as const,
      session_id: duplicate.session_id as unknown as string,
      artifact: duplicate as unknown as {
        artifact_id: string;
        session_id: string;
        strategy_id: string;
        terminal_status: string;
        created_at: string;
        [key: string]: unknown;
      },
      terminal_status: duplicate.terminal_status ?? "completed",
      idempotent: true,
      revision,
    };
  }
  if (
    input.expectRevision !== null &&
    input.expectRevision !== undefined &&
    existing.length > 0
  ) {
    const latest = [...existing].sort((a, b) =>
      a.created_at < b.created_at ? 1 : -1,
    )[0];
    const latestRevision = latest.strategy_revision
      ? Number.parseInt(latest.strategy_revision, 10)
      : 0;
    if (latestRevision !== input.expectRevision) {
      throw new Error(
        `revision_conflict: expected ${input.expectRevision} but found ${latestRevision}`,
      );
    }
  }
  // Prefer the real brain wake path for manual compaction: build a
  // BrainWakeRequest with compactionIntent and run it through the
  // selected Rust brain (chat-completions / responses). This ensures
  // the same safe-boundary check and provider projection mutation as
  // automatic compaction, rather than a synthetic CoreEngine artifact.
  // Route through the real Rust brain wake and its observed dispatch (service-wake-dispatch
  // observeWakeEvents/appendCoreEventsToChatLog). That is the only path that maps
  // provider_status artifact -> saveContextCompactionArtifact and persists
  // completed/failed. Fail closed if no persisted brain result exists; do not
  // silently fall back to the synthetic CoreEngine artifact as a success path
  // (R6624-2/3/4). The direct Rust manual operation remains available only via
  // native bridge for delegated/idempotency testing, not as the public route's
  // success path.
  // R6624-6: route through observed wake dispatch (observeWakeEvents/appendCoreEventsToChatLog)
  // so the brain's provider_status artifacts are durably persisted via saveContextCompactionArtifact.
  // This is the only path that maps brain events to durable rows; direct wakeBrain only returns
  // BrainWakeAccepted and would never persist. We build the request via the dispatch context
  // (brain, systemPrompt, roleAssembly) and then observe.
  let wakeError: unknown = undefined;
  try {
    const dispatchCtx = deps.dispatch;
    const brain = dispatchCtx.brainForProfile(
      input.session.profileId as unknown as ProfileId,
    );
    if (brain === undefined) {
      throw new Error(
        `no brain registered for profile ${String(input.session.profileId)}`,
      );
    }
    const profileContext = await dispatchCtx.loadProfileContext(
      input.session.profileId as unknown as ProfileId,
    );
    const configured = dispatchCtx.configuredSessionForRuntimeSession(
      input.session as unknown as SessionState,
    );
    const strategyPrep = await dispatchCtx.prepareContextStrategy({
      session: input.session as unknown as SessionState,
      configuredSession: configured as unknown as never,
      profileContext,
    });
    const roleplayContext = await dispatchCtx.roleplayPromptContextForSession(
      input.session as unknown as SessionState,
    );
    const effectiveProfileContext = {
      ...profileContext,
      toolSelection: effectiveToolSelectionForResourceLimits(
        profileContext.toolSelection,
        (input.session as unknown as SessionState).resourceLimits,
      ),
    };
    const roleInput = {
      sessionMemoryContext: strategyPrep.sessionMemoryContext,
      additionalInstructions: [
        ...strategyPrep.additionalInstructions,
        ...(roleplayContext ? [roleplayContext] : []),
      ],
    };
    const role = buildProfileRoleAssembly(
      effectiveProfileContext as unknown as never,
      roleInput as never,
    );
    const wakeId = `manual-compact-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    const compactionIntentPayload = {
      intentKey: input.intentKey,
      kind: "manual",
      strategyId: input.strategyId ?? undefined,
      strategyRevision: input.strategyRevision ?? undefined,
      sourceProjectionFingerprint:
        input.sourceProjectionFingerprint ?? undefined,
      trigger: "manual_intent",
    };
    const request = await dispatchCtx.bridge.buildBrainWakeRequestForSession({
      brain,
      sessionId: input.session.sessionId,
      systemPrompt: role.systemPrompt,
      roleAssemblyJson: new TextEncoder().encode(
        JSON.stringify(role.roleAssembly),
      ),
      wakeId,
      compactionIntent: compactionIntentPayload,
    } as unknown as never);
    // Also set snake_case for Rust deserialization (JsonSchema uses snake_case for BrainWakeRequest)
    (request as unknown as Record<string, unknown>).compaction_intent =
      compactionIntentPayload;
    (request as unknown as Record<string, unknown>).compactionIntent =
      compactionIntentPayload;
    console.log(
      `[manual-compact] built request compactionIntent=${JSON.stringify((request as unknown as Record<string, unknown>).compactionIntent ?? (request as unknown as Record<string, unknown>).compaction_intent)} wakeId=${wakeId}`,
    );
    const observed = await observeWakeEvents(
      dispatchCtx,
      input.session.sessionId as unknown as SessionId,
      () => dispatchCtx.bridge.wakeBrain(request as unknown as never),
      (events) =>
        appendCoreEventsToChatLog(
          dispatchCtx,
          input.session as unknown as SessionState,
          wakeId,
          events,
        ),
    );
    console.log(
      `[manual-compact] wakeId=${wakeId} observed ${observed.events.length} events accepted=${JSON.stringify(observed.accepted).slice(0, 500)} events=${JSON.stringify(observed.events).slice(0, 2000)}`,
    );
    // Poll for the persisted artifact (completed or failed) that appendCoreEventsToChatLog
    // will have written via saveContextCompactionArtifact. Use same idempotency key as Rust.
    for (let attempt = 0; attempt < 20; attempt++) {
      await new Promise((r) => setTimeout(r, 250));
      const found = await deps.bridge.listContextCompactionArtifacts({
        session_id: input.session.sessionId,
        branch_id: undefined,
        strategy_id: undefined,
        enters_future_context: undefined,
        latest_only: false,
        limit: 1000,
        offset: 0,
      });
      const match = found.find((a) =>
        isManualCompactionDuplicate(
          a,
          {
            intentKey: input.intentKey,
            sessionId: input.session.sessionId,
            sourceProjectionFingerprint: input.sourceProjectionFingerprint,
          },
          effectiveFingerprint,
        ),
      );
      if (match) {
        const rev = match.strategy_revision
          ? Number.parseInt(match.strategy_revision, 10)
          : 0;
        return {
          ok: true as const,
          session_id: match.session_id as unknown as string,
          artifact: match as unknown as {
            artifact_id: string;
            session_id: string;
            strategy_id: string;
            terminal_status: string;
            created_at: string;
            strategy_revision?: string | null;
            [key: string]: unknown;
          },
          terminal_status: match.terminal_status ?? "completed",
          idempotent: false,
          revision: rev,
        };
      }
    }
    // Also check observed events directly: if the brain emitted a compaction event but
    // appendCoreEventsToChatLog somehow missed it (should not happen), try to persist now
    // via the same mapper and then re-poll once.
    const compactionEvent = observed.events.find(
      (e) =>
        e.type === "brain_event_observed" &&
        (e.event as BrainEvent).type === "provider_status" &&
        String(
          (e.event as unknown as { metadataJson?: unknown }).metadataJson ?? "",
        ).includes("context_compaction"),
    ) as unknown as { event: BrainEvent } | undefined;
    if (compactionEvent) {
      try {
        await appendCoreEventsToChatLog(
          dispatchCtx,
          input.session as unknown as SessionState,
          wakeId,
          observed.events,
        );
        const found2 = await deps.bridge.listContextCompactionArtifacts({
          session_id: input.session.sessionId,
          branch_id: undefined,
          strategy_id: undefined,
          enters_future_context: undefined,
          latest_only: false,
          limit: 1000,
          offset: 0,
        });
        const match2 = found2.find((a) =>
          isManualCompactionDuplicate(
            a,
            {
              intentKey: input.intentKey,
              sessionId: input.session.sessionId,
              sourceProjectionFingerprint: input.sourceProjectionFingerprint,
            },
            effectiveFingerprint,
          ),
        );
        if (match2) {
          const rev = match2.strategy_revision
            ? Number.parseInt(match2.strategy_revision, 10)
            : 0;
          return {
            ok: true as const,
            session_id: match2.session_id as unknown as string,
            artifact: match2 as unknown as {
              artifact_id: string;
              session_id: string;
              strategy_id: string;
              terminal_status: string;
              created_at: string;
              strategy_revision?: string | null;
              [key: string]: unknown;
            },
            terminal_status: match2.terminal_status ?? "completed",
            idempotent: false,
            revision: rev,
          };
        }
      } catch {}
    }
    // No durable artifact persisted – fail closed per R6624-4/5/6, do not fall back to synthetic success.
    throw new Error(
      `manual compaction did not produce a durable brain artifact for intent ${input.intentKey} (wakeId ${wakeId}); not falling back to synthetic CoreEngine artifact`,
    );
  } catch (error) {
    wakeError = error;
    // Only fall through to synthetic when the brain wake could not even be constructed
    // (e.g., no brain registered for session's profile). If a wake was attempted and simply
    // produced no durable artifact, we fail closed per R6624-5/6.
    const msg = error instanceof Error ? error.message : String(error);
    const isNoBrain =
      msg.includes("no brain") ||
      msg.includes("brain not") ||
      msg.includes("No brain") ||
      msg.includes("unknown profile") ||
      msg.includes("not registered") ||
      msg.includes("Failed to convert napi value Undefined") ||
      msg.includes("Failed to convert napi value") ||
      msg.includes("napi value Undefined");
    if (!isNoBrain) {
      throw error;
    }
  }
  // Synthetic fallback only for explicitly proven no-brain/native fallback boundary (R6624-5).
  if (wakeError !== undefined) {
    const msg2 =
      wakeError instanceof Error ? wakeError.message : String(wakeError);
    const isNoBrain2 =
      msg2.includes("no brain") ||
      msg2.includes("brain not") ||
      msg2.includes("No brain") ||
      msg2.includes("unknown profile") ||
      msg2.includes("not registered") ||
      msg2.includes("Failed to convert napi value Undefined") ||
      msg2.includes("Failed to convert napi value") ||
      msg2.includes("napi value Undefined");
    if (!isNoBrain2) throw wakeError;
  }
  const typed = await deps.bridge.manualContextCompaction({
    sessionId: input.session.sessionId,
    intentKey: input.intentKey,
    strategyId: input.strategyId,
    strategyRevision: input.strategyRevision,
    sourceProjectionFingerprint: input.sourceProjectionFingerprint,
    expectRevision: input.expectRevision ?? null,
  });
  const artifact = {
    ...typed.artifact,
    terminal_status: typed.artifact.terminal_status ?? typed.terminalStatus,
  };
  return {
    ok: true as const,
    session_id: typed.artifact.session_id,
    artifact,
    terminal_status: typed.terminalStatus,
    idempotent: typed.idempotent,
    revision: typed.revision,
  };
}
