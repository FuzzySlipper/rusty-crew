import type {
  ProfileId,
  ScheduledRunSummary,
  SessionState,
} from "@rusty-crew/contracts";
import type {
  NativeBridgeModule,
  NativeProfileMemoryRecord,
} from "@rusty-crew/native-bridge";
import type { AdminDiagnosticsContext } from "./admin-diagnostics-api.js";
import {
  runBackgroundMemorySkillReview,
  type BackgroundReviewPayload,
  type BackgroundReviewResult,
} from "./background-memory-skill-review.js";
import { runStructuredCaptureProvider } from "./capture-producer-provider.js";
import type { CaptureMemoryProposalPlan } from "./capture-memory-proposals.js";
import {
  buildProfileRoleAssembly,
  type ProfileRoleAssemblyResult,
} from "./profile-role-assembly.js";
import type { LoadedProfileContext } from "./profile-loading.js";
import {
  runScheduledHostExecutors,
  type ScheduledHostExecutorContext,
} from "./scheduled-host-executors.js";
import { buildToolContextDiagnosticsReport } from "./tool-context-diagnostics.js";
import { buildToolRegistryDiagnostics } from "./tool-registry-diagnostics.js";
import type { AdapterDiagnosticsProjection } from "./adapter-diagnostics.js";
import type { RustyCrewRuntimeConfig } from "./service-runtime-config.js";
import { effectiveToolSelectionForResourceLimits } from "./tool-profile-selection.js";

export interface ServiceBackgroundReviewEvent {
  source: string;
  eventType: string;
  summary: string;
  severity?: string;
  workRef?: Record<string, unknown>;
  resultRef?: Record<string, unknown>;
}

export interface ServiceBackgroundReviewStateUpdate {
  lastRunAt?: string;
  lastError?: string;
  recentFindings?: number;
  lastCaptureProposalCount?: number;
  lastPersistedCaptureProposalCount?: number;
  lastSkippedReasons?: readonly string[];
}

export interface ServiceBackgroundReviewContext {
  bridge: Pick<
    NativeBridgeModule,
    | "claimScheduledHostRuns"
    | "completeScheduledHostRun"
    | "getModelProvider"
    | "getModelProviderSecret"
    | "listProfileMemory"
    | "listSessionActivityDigests"
    | "listSessions"
    | "planCaptureMemoryProposals"
    | "saveMemoryProposal"
  >;
  get runtimeConfig(): RustyCrewRuntimeConfig;
  diagnostics(): Promise<AdminDiagnosticsContext>;
  loadProfileContext(profileId: ProfileId): Promise<LoadedProfileContext>;
  buildAdapterDiagnostics(
    now: string,
  ): AdapterDiagnosticsProjection | undefined;
  denMemoryConfigured(): boolean;
  now(): string;
  updateBackgroundReviewState(update: ServiceBackgroundReviewStateUpdate): void;
  recordEvent(event: ServiceBackgroundReviewEvent): void;
}

export function scheduledHostExecutorContext(
  context: ServiceBackgroundReviewContext,
): ScheduledHostExecutorContext {
  return {
    bridge: context.bridge,
    diagnostics: () => context.diagnostics(),
    jobPayload: (run) => configuredScheduledJobPayload(context, run.jobId),
    backgroundReview: (run, payload) =>
      runServiceBackgroundReview(context, run, payload),
  };
}

export function configuredScheduledJobPayload(
  context: Pick<ServiceBackgroundReviewContext, "runtimeConfig">,
  jobId: string,
): unknown {
  return context.runtimeConfig.scheduledJobs.find((job) => job.id === jobId)
    ?.payload;
}

export async function runServiceBackgroundReview(
  context: ServiceBackgroundReviewContext,
  run: ScheduledRunSummary,
  payload: BackgroundReviewPayload,
): Promise<BackgroundReviewResult> {
  try {
    const now = context.now();
    const profileId = String(payload.profileId);
    const profileContext = await context.loadProfileContext(
      profileId as ProfileId,
    );
    const sessions = await context.bridge.listSessions().catch(() => []);
    const session =
      sessions.find((candidate) => candidate.profileId === profileId) ??
      configuredSessionForProfile(context.runtimeConfig, profileId);
    if (!session) {
      throw new Error(`no configured session found for profile ${profileId}`);
    }
    const denseProfileMemory =
      payload.includeDenseProfileMemory === false
        ? []
        : await context.bridge
            .listProfileMemory({
              profileId,
              limit: payload.maxCandidates ?? 100,
            })
            .catch(() => []);
    const sessionActivityDigests = await context.bridge
      .listSessionActivityDigests({
        profile_id: profileId as ProfileId,
        include_reviewed: false,
        limit: payload.maxCandidates ?? 100,
        offset: 0,
      })
      .catch(() => []);
    const role = buildProfileRoleAssembly(profileContext, {
      includeSkillBodies: false,
    });
    const diagnostics = backgroundToolDiagnostics({
      context,
      now,
      session,
      profileContext,
      role,
      denseProfileMemoryCount: denseProfileMemory.length,
    });
    const result = await runBackgroundMemorySkillReview({
      runId: String(run.runId),
      now,
      payload,
      diagnostics,
      skills: profileContext.skills,
      denseProfileMemory: denseProfileMemory.map(toBackgroundMemoryRecord),
      sessionActivityDigests,
      captureProvider: (captureInput) =>
        runStructuredCaptureProvider({
          ...captureInput,
          bridge: context.bridge,
        }),
      capturePlanner: (captureInput) => {
        const request: {
          run_id: string;
          profile_id: string;
          allowed_spaces: string[];
          max_proposals?: number;
          candidates: typeof captureInput.proposals;
        } = {
          run_id: captureInput.runId,
          profile_id: captureInput.profileId.toString(),
          allowed_spaces: ["profile_dense"],
          candidates: captureInput.proposals,
        };
        if (captureInput.maxProposals !== undefined) {
          request.max_proposals = captureInput.maxProposals;
        }
        return context.bridge.planCaptureMemoryProposals(
          request,
        ) as Promise<CaptureMemoryProposalPlan>;
      },
    });
    const persistedCaptureProposalCount =
      await persistBackgroundReviewProposals(context, result);
    context.updateBackgroundReviewState({
      lastRunAt: result.finishedAt,
      lastError: undefined,
      recentFindings: result.findingCount,
      lastCaptureProposalCount: result.findings.filter(
        (finding) => finding.memoryProposal !== undefined,
      ).length,
      lastPersistedCaptureProposalCount: persistedCaptureProposalCount,
      lastSkippedReasons: result.skippedReasons,
    });
    context.recordEvent({
      source: "background-review",
      eventType: "memory_skills_review_completed",
      summary: `Background ${result.reviewType} review for ${result.profileId} produced ${result.findingCount} finding(s) and persisted ${persistedCaptureProposalCount} capture proposal(s).`,
    });
    return result;
  } catch (error) {
    const lastError = errorMessage(error, "background review failed");
    context.updateBackgroundReviewState({ lastError });
    context.recordEvent({
      source: "background-review",
      eventType: "memory_skills_review_failed",
      summary: lastError,
      severity: "warning",
    });
    throw error;
  }
}

export async function runScheduledBackgroundReviewHost(
  context: ServiceBackgroundReviewContext,
) {
  return runScheduledHostExecutors(scheduledHostExecutorContext(context));
}

async function persistBackgroundReviewProposals(
  context: ServiceBackgroundReviewContext,
  result: BackgroundReviewResult,
): Promise<number> {
  if (result.dryRun) return 0;
  let persisted = 0;
  for (const finding of result.findings) {
    if (finding.memoryProposal === undefined) continue;
    try {
      await context.bridge.saveMemoryProposal(finding.memoryProposal);
      persisted += 1;
    } catch (error) {
      context.recordEvent({
        source: "background-review",
        eventType: "capture_proposal_persist_failed",
        severity: "warning",
        summary: errorMessage(error, "capture proposal persist failed"),
      });
    }
  }
  return persisted;
}

function backgroundToolDiagnostics(input: {
  context: ServiceBackgroundReviewContext;
  now: string;
  session: SessionState | RustyCrewRuntimeConfig["sessions"][number];
  profileContext: LoadedProfileContext;
  role: ProfileRoleAssemblyResult;
  denseProfileMemoryCount: number;
}) {
  const effectiveToolSelection = effectiveToolSelectionForResourceLimits(
    input.profileContext.toolSelection,
    input.session.resourceLimits,
  );
  const toolDiagnostics = buildToolRegistryDiagnostics({
    catalogId: effectiveToolSelection.catalogId,
    inventoryRequest: {
      requestedTools: effectiveToolSelection.toolProfile.tools.map(
        (tool) => tool.name,
      ),
    },
  });
  return buildToolContextDiagnosticsReport({
    now: input.now,
    session: {
      sessionId: input.session.sessionId,
      agentId: input.session.agentId,
      profileId: input.session.profileId,
      kind: input.session.kind,
    },
    toolDiagnostics,
    toolSelection: effectiveToolSelection,
    profileContext: input.profileContext,
    toolPolicy: input.profileContext.profile.toolPolicy,
    roleAssembly: input.role.roleAssembly,
    systemPrompt: input.role.systemPrompt,
    resourceLimits: input.session.resourceLimits,
    adapters: input.context.buildAdapterDiagnostics(input.now),
    memorySkillsPlanning: {
      denMemory: {
        configured: input.context.denMemoryConfigured(),
        clientAvailable: input.context.denMemoryConfigured(),
        mode: "metadata",
        endpointConfigured: input.context.denMemoryConfigured(),
      },
      skills: {
        rootConfigured: Boolean(input.context.runtimeConfig.skillsDir),
        rootReadable: true,
        profileSkillCount: input.profileContext.profile.skills?.length ?? 0,
        loadedSkillCount: input.profileContext.skills.length,
        missingSkillCount: Math.max(
          0,
          (input.profileContext.profile.skills?.length ?? 0) -
            input.profileContext.skills.length,
        ),
        invalidSkillCount: 0,
      },
      denseProfileMemory: {
        clientAvailable: true,
        recordCount: input.denseProfileMemoryCount,
      },
      sessionSearch: { available: true },
      todo: { available: true },
      counters: { available: true, resetAllowed: false },
    },
  });
}

function configuredSessionForProfile(
  runtimeConfig: RustyCrewRuntimeConfig,
  profileId: string,
): RustyCrewRuntimeConfig["sessions"][number] | undefined {
  return runtimeConfig.sessions.find(
    (session) => session.profileId === profileId,
  );
}

function toBackgroundMemoryRecord(record: NativeProfileMemoryRecord) {
  return {
    profileId: record.profileId,
    key: record.key,
    content: record.content,
    revision: record.revision,
    updatedAt: record.updatedAt,
    metadata: parseJson(record.metadataJson),
  };
}

function parseJson(value: string): unknown {
  try {
    return JSON.parse(value) as unknown;
  } catch {
    return {};
  }
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}
