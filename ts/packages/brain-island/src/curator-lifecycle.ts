import { stat } from "node:fs/promises";
import { join } from "node:path";
import {
  curatorSkillSourceRef,
  type CuratorCandidateLifecycle,
  type CuratorStoredCandidate,
  type MemoryCuratorGovernanceStore,
} from "./curator-mutations.js";

export interface CuratorLifecyclePolicy {
  staleAfterMs: number;
  archiveAfterMs: number;
}

export type RustCuratorLifecycleTarget =
  | "active"
  | "stale"
  | "archived"
  | "skipped"
  | "unchanged";

export interface RustCuratorLifecyclePlanInput {
  candidate: {
    candidate_id: string;
    status: "proposed" | "previewed" | "approved" | "applied";
    lifecycle_state?: "active" | "stale" | "archived";
    lifecycle_reason_code?: string;
    age_since_activity_ms?: number;
    stale_duration_ms?: number;
    activity_after_stale: boolean;
    source_current: boolean;
    source_current_reason_code?: string;
    pinned: boolean;
  };
  now: string;
  policy: {
    stale_after_ms: number;
    archive_after_ms: number;
  };
}

export interface RustCuratorLifecyclePlan {
  accepted: boolean;
  candidate_id: string;
  from?: "active" | "stale" | "archived";
  to: RustCuratorLifecycleTarget;
  reason_code?: string;
  resulting_lifecycle_state?: "active" | "stale" | "archived";
  audit_ref?: string;
  receipt_id: string;
  diagnostics: readonly {
    reason_code: string;
    message: string;
  }[];
}

export type CuratorLifecyclePlanner = (
  input: RustCuratorLifecyclePlanInput,
) => Promise<RustCuratorLifecyclePlan> | RustCuratorLifecyclePlan;

export interface CuratorLifecycleTransition {
  candidateId: string;
  targetRef: string;
  from: "active" | "stale" | "archived";
  to: "active" | "stale" | "archived" | "skipped";
  reasonCode: string;
}

export interface CuratorLifecycleReport {
  checkedAt: string;
  staleAfterMs: number;
  archiveAfterMs: number;
  active: number;
  stale: number;
  archived: number;
  reactivated: number;
  pinnedSkipped: number;
  unchanged: number;
  transitions: readonly CuratorLifecycleTransition[];
}

export async function runCuratorLifecycleTransitions(input: {
  store: MemoryCuratorGovernanceStore;
  skillsDir: string;
  now: string;
  planner: CuratorLifecyclePlanner;
  policy?: Partial<CuratorLifecyclePolicy>;
}): Promise<CuratorLifecycleReport> {
  const policy = {
    staleAfterMs: input.policy?.staleAfterMs ?? 24 * 60 * 60 * 1_000,
    archiveAfterMs: input.policy?.archiveAfterMs ?? 7 * 24 * 60 * 60 * 1_000,
  };
  const transitions: CuratorLifecycleTransition[] = [];
  let active = 0;
  let stale = 0;
  let archived = 0;
  let reactivated = 0;
  let pinnedSkipped = 0;
  let unchanged = 0;

  for (const stored of input.store.candidates.values()) {
    const lifecycle = normalizedLifecycle(stored);
    const plan = await input.planner(
      await rustLifecyclePlanInput(input, stored, lifecycle, policy),
    );
    if (!plan.accepted) {
      transitions.push(
        transition(
          stored,
          "skipped",
          plan.diagnostics[0]?.reason_code ?? "curator_lifecycle_rejected",
        ),
      );
      unchanged += 1;
      continue;
    }
    switch (plan.to) {
      case "active":
        input.store.updateCandidateLifecycle(
          stored.candidate.candidateId,
          activeLifecycle(
            input.now,
            plan.reason_code ?? "candidate_reactivated",
          ),
        );
        active += 1;
        reactivated += 1;
        transitions.push(
          transition(
            stored,
            "active",
            plan.reason_code ?? "candidate_reactivated",
          ),
        );
        break;
      case "stale":
        input.store.updateCandidateLifecycle(
          stored.candidate.candidateId,
          staleLifecycle(input.now, plan.reason_code ?? "candidate_stale"),
        );
        stale += 1;
        transitions.push(
          transition(stored, "stale", plan.reason_code ?? "candidate_stale"),
        );
        break;
      case "archived":
        input.store.updateCandidateLifecycle(
          stored.candidate.candidateId,
          archivedLifecycle(input.now, plan.reason_code ?? "idle_stale"),
        );
        archived += 1;
        transitions.push(
          transition(
            stored,
            "archived",
            plan.reason_code ?? "candidate_stale_archive_due",
          ),
        );
        break;
      case "skipped":
        if (plan.reason_code === "skill_pinned") pinnedSkipped += 1;
        unchanged += 1;
        transitions.push(
          transition(
            stored,
            "skipped",
            plan.reason_code ?? "candidate_skipped",
          ),
        );
        break;
      case "unchanged":
        switch (plan.resulting_lifecycle_state) {
          case "active":
            active += 1;
            break;
          case "stale":
            stale += 1;
            break;
          case "archived":
            archived += 1;
            break;
        }
        unchanged += 1;
        break;
    }
  }

  return {
    checkedAt: input.now,
    staleAfterMs: policy.staleAfterMs,
    archiveAfterMs: policy.archiveAfterMs,
    active,
    stale,
    archived,
    reactivated,
    pinnedSkipped,
    unchanged,
    transitions,
  };
}

function normalizedLifecycle(
  stored: CuratorStoredCandidate,
): CuratorCandidateLifecycle {
  return stored.lifecycle ?? { state: "active" };
}

function staleLifecycle(
  now: string,
  reasonCode: string,
): CuratorCandidateLifecycle {
  return {
    state: "stale",
    reasonCode,
    staleAt: now,
    lastTransitionAt: now,
  };
}

function activeLifecycle(
  now: string,
  reasonCode: string,
): CuratorCandidateLifecycle {
  return {
    state: "active",
    reasonCode,
    reactivatedAt: now,
    lastTransitionAt: now,
  };
}

function archivedLifecycle(
  now: string,
  reasonCode: string,
): CuratorCandidateLifecycle {
  return {
    state: "archived",
    reasonCode,
    archivedAt: now,
    lastTransitionAt: now,
  };
}

function transition(
  stored: CuratorStoredCandidate,
  to: CuratorLifecycleTransition["to"],
  reasonCode: string,
): CuratorLifecycleTransition {
  return {
    candidateId: stored.candidate.candidateId,
    targetRef: stored.candidate.targetRef,
    from: stored.lifecycle?.state ?? "active",
    to,
    reasonCode,
  };
}

function latestActivityAt(stored: CuratorStoredCandidate): string | undefined {
  return maxIso(stored.previewedAt, stored.approval?.approvedAt);
}

async function rustLifecyclePlanInput(
  input: {
    store: MemoryCuratorGovernanceStore;
    skillsDir: string;
    now: string;
  },
  stored: CuratorStoredCandidate,
  lifecycle: CuratorCandidateLifecycle,
  policy: CuratorLifecyclePolicy,
): Promise<RustCuratorLifecyclePlanInput> {
  const slug = skillSlugFromTarget(stored.candidate.targetRef);
  const currentness = await candidateSourcesCurrent(input.skillsDir, stored);
  const latestActivity = latestActivityAt(stored);
  const activityAt =
    latestActivity ??
    input.store.batches.get(stored.candidate.batchId)?.generatedAt;
  return {
    candidate: {
      candidate_id: stored.candidate.candidateId,
      status: curatorStoredStatus(stored.status),
      ...(stored.lifecycle?.state
        ? { lifecycle_state: stored.lifecycle.state }
        : {}),
      ...(lifecycle.reasonCode
        ? { lifecycle_reason_code: lifecycle.reasonCode }
        : {}),
      ...(activityAt
        ? { age_since_activity_ms: elapsedMs(activityAt, input.now) }
        : {}),
      ...(lifecycle.staleAt
        ? { stale_duration_ms: elapsedMs(lifecycle.staleAt, input.now) }
        : {}),
      activity_after_stale: Boolean(
        latestActivity &&
        lifecycle.staleAt &&
        latestActivity > lifecycle.staleAt,
      ),
      source_current: currentness.sourceCurrent,
      ...(currentness.reasonCode
        ? { source_current_reason_code: currentness.reasonCode }
        : {}),
      pinned: Boolean(slug && (await isPinnedSkill(input.skillsDir, slug))),
    },
    now: input.now,
    policy: {
      stale_after_ms: policy.staleAfterMs,
      archive_after_ms: policy.archiveAfterMs,
    },
  };
}

function curatorStoredStatus(
  status: CuratorStoredCandidate["status"],
): RustCuratorLifecyclePlanInput["candidate"]["status"] {
  if (status === "previewed") return "previewed";
  if (status === "approved") return "approved";
  if (status === "applied") return "applied";
  return "proposed";
}

async function candidateSourcesCurrent(
  skillsDir: string,
  stored: CuratorStoredCandidate,
): Promise<{ sourceCurrent: boolean; reasonCode?: string }> {
  for (const ref of stored.candidate.sourceRefs) {
    if (ref.kind !== "skill" || !ref.hash) continue;
    try {
      const current = await curatorSkillSourceRef(skillsDir, ref.ref);
      if (current.hash !== ref.hash) {
        return { sourceCurrent: false, reasonCode: "source_changed" };
      }
    } catch {
      return { sourceCurrent: false, reasonCode: "source_changed" };
    }
  }
  return { sourceCurrent: true };
}

async function isPinnedSkill(
  skillsDir: string,
  slug: string,
): Promise<boolean> {
  return (
    (await pathExists(join(skillsDir, `${slug}.pinned`))) ||
    (await pathExists(join(skillsDir, `${slug}.d`, ".pinned"))) ||
    (await pathExists(join(skillsDir, slug, ".pinned")))
  );
}

async function pathExists(path: string): Promise<boolean> {
  try {
    await stat(path);
    return true;
  } catch {
    return false;
  }
}

function skillSlugFromTarget(targetRef: string): string | undefined {
  return targetRef.startsWith("skill:")
    ? targetRef.slice("skill:".length)
    : undefined;
}

function elapsedMs(start: string | undefined, end: string): number {
  if (!start) return 0;
  const startMs = Date.parse(start);
  const endMs = Date.parse(end);
  if (!Number.isFinite(startMs) || !Number.isFinite(endMs)) return 0;
  return Math.max(0, endMs - startMs);
}

function maxIso(
  left: string | undefined,
  right: string | undefined,
): string | undefined {
  if (!left) return right;
  if (!right) return left;
  return left > right ? left : right;
}
