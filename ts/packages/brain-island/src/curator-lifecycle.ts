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
    if (stored.status === "applied") {
      unchanged += 1;
      continue;
    }
    const slug = skillSlugFromTarget(stored.candidate.targetRef);
    if (slug && (await isPinnedSkill(input.skillsDir, slug))) {
      pinnedSkipped += 1;
      transitions.push(transition(stored, "skipped", "skill_pinned"));
      continue;
    }

    const lifecycle = normalizedLifecycle(stored);
    if (lifecycle.state === "archived") {
      archived += 1;
      unchanged += 1;
      continue;
    }

    const current = await candidateSourcesCurrent(input.skillsDir, stored);
    if (lifecycle.state === "stale") {
      const activityAt = latestActivityAt(stored);
      if (current && activityAt && activityAt > (lifecycle.staleAt ?? "")) {
        input.store.updateCandidateLifecycle(
          stored.candidate.candidateId,
          activeLifecycle(input.now, "candidate_reactivated"),
        );
        active += 1;
        reactivated += 1;
        transitions.push(transition(stored, "active", "candidate_reactivated"));
        continue;
      }
      if (elapsedMs(lifecycle.staleAt, input.now) >= policy.archiveAfterMs) {
        input.store.updateCandidateLifecycle(
          stored.candidate.candidateId,
          archivedLifecycle(input.now, lifecycle.reasonCode ?? "idle_stale"),
        );
        archived += 1;
        transitions.push(
          transition(stored, "archived", "candidate_stale_archive_due"),
        );
        continue;
      }
      stale += 1;
      unchanged += 1;
      continue;
    }

    const reasonCode = current
      ? idleReason(input.store, stored, input.now, policy)
      : "source_changed";
    if (reasonCode) {
      input.store.updateCandidateLifecycle(
        stored.candidate.candidateId,
        staleLifecycle(input.now, reasonCode),
      );
      stale += 1;
      transitions.push(transition(stored, "stale", reasonCode));
      continue;
    }

    active += 1;
    unchanged += 1;
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

function idleReason(
  store: MemoryCuratorGovernanceStore,
  stored: CuratorStoredCandidate,
  now: string,
  policy: CuratorLifecyclePolicy,
): string | undefined {
  const lastActivityAt =
    latestActivityAt(stored) ??
    store.batches.get(stored.candidate.batchId)?.generatedAt;
  if (elapsedMs(lastActivityAt, now) >= policy.staleAfterMs) {
    return "candidate_idle_stale";
  }
  return undefined;
}

async function candidateSourcesCurrent(
  skillsDir: string,
  stored: CuratorStoredCandidate,
): Promise<boolean> {
  for (const ref of stored.candidate.sourceRefs) {
    if (ref.kind !== "skill" || !ref.hash) continue;
    try {
      const current = await curatorSkillSourceRef(skillsDir, ref.ref);
      if (current.hash !== ref.hash) return false;
    } catch {
      return false;
    }
  }
  return true;
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
