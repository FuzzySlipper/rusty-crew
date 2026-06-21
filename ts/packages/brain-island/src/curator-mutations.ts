import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  writeFileSync,
} from "node:fs";
import { cp, mkdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import { dirname, join, resolve, sep } from "node:path";
import type {
  CuratorExecuteReceipt,
  CuratorExecuteRequest,
} from "./planning-tools.js";
import { CuratorExecuteError } from "./planning-tools.js";
import { loadSkill } from "./profile-loading.js";
import { skillManageTool, type SkillManagementResult } from "./skills-tools.js";
import type {
  CuratorCandidate,
  CuratorCandidateBatch,
  CuratorCandidateSourceRef,
} from "./curator-candidates.js";

export type CuratorMutationOperation =
  | {
      type: "skill_patch";
      slug: string;
      content?: string;
      oldString?: string;
      newString?: string;
    }
  | {
      type: "skill_create";
      slug: string;
      content: string;
    }
  | {
      type: "skill_archive";
      slug: string;
      absorbedInto: string;
    }
  | {
      type: "sidecar_write";
      slug: string;
      filePath: string;
      fileContent: string;
    };

export interface CuratorMutationCandidate extends CuratorCandidate {
  mutation: CuratorMutationOperation;
  expiresAt?: string;
}

export interface CuratorApprovalRecord {
  candidateId: string;
  actorId?: string;
  reason: string;
  approvedAt: string;
  fingerprint: string;
}

export type CuratorMutationStatus =
  | "applied"
  | "failed"
  | "rolled_back"
  | "rollback_failed";

export interface CuratorSnapshotRef {
  snapshotId: string;
  snapshotDir: string;
  createdAt: string;
  skillPath: string;
  skillExisted: boolean;
  skillSnapshotPath?: string;
  sidecarPath?: string;
  sidecarExisted?: boolean;
  sidecarSnapshotPath?: string;
  filePath?: string;
  fileExisted?: boolean;
  fileSnapshotPath?: string;
}

export interface CuratorMutationRecord {
  mutationId: string;
  candidateId: string;
  action: CuratorMutationOperation["type"];
  actorId?: string;
  reason: string;
  appliedAt: string;
  status: CuratorMutationStatus;
  snapshot: CuratorSnapshotRef;
  rollbackRef?: string;
  changedPaths: readonly string[];
  management?: SkillManagementResult;
  error?: string;
}

export interface CuratorGovernanceExecutorOptions {
  skillsDir: string;
  store: MemoryCuratorGovernanceStore;
  snapshotDir?: string;
  now?: () => Date;
  scan?: (
    request: CuratorExecuteRequest,
  ) => Promise<CuratorCandidateBatch> | CuratorCandidateBatch;
}

export type CuratorStoredCandidateStatus =
  | CuratorCandidate["status"]
  | "previewed"
  | "approved"
  | "applied";

export type CuratorCandidateLifecycleState = "active" | "stale" | "archived";

export interface CuratorCandidateLifecycle {
  state: CuratorCandidateLifecycleState;
  reasonCode?: string;
  lastTransitionAt?: string;
  staleAt?: string;
  archivedAt?: string;
  reactivatedAt?: string;
}

export interface CuratorStoredCandidate {
  candidate: CuratorMutationCandidate;
  approval?: CuratorApprovalRecord;
  previewedAt?: string;
  status: CuratorStoredCandidateStatus;
  lifecycle?: CuratorCandidateLifecycle;
}

export interface CuratorGovernanceStoreSnapshot {
  schemaVersion: 1;
  batches: readonly CuratorCandidateBatch[];
  candidates: readonly CuratorStoredCandidate[];
  mutations: readonly CuratorMutationRecord[];
}

export class MemoryCuratorGovernanceStore {
  readonly batches = new Map<string, CuratorCandidateBatch>();
  readonly candidates = new Map<string, CuratorStoredCandidate>();
  readonly mutations = new Map<string, CuratorMutationRecord>();

  constructor(snapshot?: CuratorGovernanceStoreSnapshot) {
    if (snapshot) {
      this.replaceSnapshot(snapshot);
    }
  }

  upsertBatch(
    batch: CuratorCandidateBatch,
    mutationCandidates: readonly CuratorMutationCandidate[] = [],
  ): void {
    this.batches.set(batch.batchId, batch);
    for (const candidate of mutationCandidates) {
      this.upsertCandidate(candidate);
    }
  }

  upsertCandidate(candidate: CuratorMutationCandidate): void {
    const existing = this.candidates.get(candidate.candidateId);
    this.candidates.set(candidate.candidateId, {
      candidate,
      approval: existing?.approval,
      previewedAt: existing?.previewedAt,
      status: existing?.status ?? candidate.status,
    });
  }

  getCandidate(candidateId: string): CuratorStoredCandidate | undefined {
    return this.candidates.get(candidateId);
  }

  approve(
    candidateId: string,
    approval: CuratorApprovalRecord,
  ): CuratorStoredCandidate {
    const stored = requiredCandidate(this, candidateId);
    const next = {
      ...stored,
      approval,
      status: "approved" as const,
    };
    this.candidates.set(candidateId, next);
    return next;
  }

  recordPreview(
    candidateId: string,
    previewedAt: string,
  ): CuratorStoredCandidate {
    const stored = requiredCandidate(this, candidateId);
    const next = {
      ...stored,
      previewedAt,
      status: "previewed" as const,
    };
    this.candidates.set(candidateId, next);
    return next;
  }

  recordApplied(record: CuratorMutationRecord): void {
    this.mutations.set(record.mutationId, record);
    const stored = requiredCandidate(this, record.candidateId);
    this.candidates.set(record.candidateId, {
      ...stored,
      status: "applied",
    });
  }

  recordRollback(mutationId: string, status: CuratorMutationStatus): void {
    const existing = this.mutations.get(mutationId);
    if (!existing) throw new CuratorExecuteError("curator_mutation_not_found");
    this.mutations.set(mutationId, {
      ...existing,
      status,
      rollbackRef: `curator-rollback:${mutationId}`,
    });
  }

  updateCandidateLifecycle(
    candidateId: string,
    lifecycle: CuratorCandidateLifecycle,
  ): CuratorStoredCandidate {
    const stored = requiredCandidate(this, candidateId);
    const next = {
      ...stored,
      lifecycle,
    };
    this.candidates.set(candidateId, next);
    return next;
  }

  snapshot(): CuratorGovernanceStoreSnapshot {
    return {
      schemaVersion: 1,
      batches: [...this.batches.values()],
      candidates: [...this.candidates.values()],
      mutations: [...this.mutations.values()],
    };
  }

  protected replaceSnapshot(snapshot: CuratorGovernanceStoreSnapshot): void {
    if (snapshot.schemaVersion !== 1) {
      throw new CuratorExecuteError("curator_store_schema_unsupported");
    }
    this.batches.clear();
    this.candidates.clear();
    this.mutations.clear();
    for (const batch of snapshot.batches) {
      this.batches.set(batch.batchId, batch);
    }
    for (const candidate of snapshot.candidates) {
      this.candidates.set(candidate.candidate.candidateId, candidate);
    }
    for (const mutation of snapshot.mutations) {
      this.mutations.set(mutation.mutationId, mutation);
    }
  }
}

export class FileCuratorGovernanceStore extends MemoryCuratorGovernanceStore {
  constructor(readonly stateFilePath: string) {
    super(loadGovernanceSnapshot(stateFilePath));
  }

  override upsertBatch(
    batch: CuratorCandidateBatch,
    mutationCandidates: readonly CuratorMutationCandidate[] = [],
  ): void {
    super.upsertBatch(batch, mutationCandidates);
    this.persist();
  }

  override upsertCandidate(candidate: CuratorMutationCandidate): void {
    super.upsertCandidate(candidate);
    this.persist();
  }

  override approve(
    candidateId: string,
    approval: CuratorApprovalRecord,
  ): CuratorStoredCandidate {
    const stored = super.approve(candidateId, approval);
    this.persist();
    return stored;
  }

  override recordPreview(
    candidateId: string,
    previewedAt: string,
  ): CuratorStoredCandidate {
    const stored = super.recordPreview(candidateId, previewedAt);
    this.persist();
    return stored;
  }

  override recordApplied(record: CuratorMutationRecord): void {
    super.recordApplied(record);
    this.persist();
  }

  override recordRollback(
    mutationId: string,
    status: CuratorMutationStatus,
  ): void {
    super.recordRollback(mutationId, status);
    this.persist();
  }

  override updateCandidateLifecycle(
    candidateId: string,
    lifecycle: CuratorCandidateLifecycle,
  ): CuratorStoredCandidate {
    const stored = super.updateCandidateLifecycle(candidateId, lifecycle);
    this.persist();
    return stored;
  }

  private persist(): void {
    writeGovernanceSnapshot(this.stateFilePath, this.snapshot());
  }
}

export function createCuratorGovernanceExecutor(
  options: CuratorGovernanceExecutorOptions,
): (request: CuratorExecuteRequest) => Promise<CuratorExecuteReceipt> {
  return async (request) => executeCuratorGovernanceRequest(options, request);
}

export async function executeCuratorGovernanceRequest(
  options: CuratorGovernanceExecutorOptions,
  request: CuratorExecuteRequest,
): Promise<CuratorExecuteReceipt> {
  const now = (options.now?.() ?? new Date()).toISOString();
  switch (request.action) {
    case "request_scan": {
      if (!options.scan) {
        return receipt(request, "requested", {
          summary: `scan requested for ${request.scopeType}:${request.scopeId}`,
        });
      }
      const batch = await options.scan(request);
      options.store.upsertBatch(batch);
      return receipt(request, "requested", {
        auditRef: batch.reportId,
        summary: `scan produced ${batch.candidateCount} candidate(s)`,
      });
    }
    case "preview_candidate": {
      const stored = requiredCandidate(options.store, request.candidateId);
      assertCandidateNotArchived(stored);
      await assertCandidateCurrent(options.skillsDir, stored.candidate);
      const management = await runSkillMutation(
        options,
        stored.candidate,
        true,
      );
      options.store.recordPreview(stored.candidate.candidateId, now);
      return receipt(request, "previewed", {
        auditRef: `curator-preview:${stored.candidate.candidateId}`,
        summary: summarizeManagement("preview", management),
      });
    }
    case "approve_candidate": {
      const stored = requiredCandidate(options.store, request.candidateId);
      assertCandidateNotArchived(stored);
      assertNotExpired(stored.candidate, now);
      await assertCandidateCurrent(options.skillsDir, stored.candidate);
      options.store.approve(stored.candidate.candidateId, {
        candidateId: stored.candidate.candidateId,
        actorId: request.actorId,
        reason: request.reason ?? "curator approval",
        approvedAt: now,
        fingerprint: stored.candidate.fingerprint,
      });
      return receipt(request, "approved", {
        auditRef: `curator-approval:${stored.candidate.candidateId}`,
        summary: `approved ${stored.candidate.summary}`,
      });
    }
    case "apply_candidate": {
      const stored = requiredCandidate(options.store, request.candidateId);
      assertCandidateNotArchived(stored);
      assertNotExpired(stored.candidate, now);
      await assertCandidateCurrent(options.skillsDir, stored.candidate);
      if (request.dryRun) {
        const management = await runSkillMutation(
          options,
          stored.candidate,
          true,
        );
        return receipt(request, "previewed", {
          auditRef: `curator-preview:${stored.candidate.candidateId}`,
          summary: summarizeManagement("dry-run apply", management),
        });
      }
      if (!stored.approval) {
        throw new CuratorExecuteError("curator_candidate_not_approved");
      }
      if (stored.approval.fingerprint !== stored.candidate.fingerprint) {
        throw new CuratorExecuteError("curator_approval_stale");
      }
      const snapshot = await snapshotBeforeMutation(options, stored.candidate);
      const management = await runSkillMutation(
        options,
        stored.candidate,
        false,
      );
      if (!management.changed) {
        throw new CuratorExecuteError("curator_mutation_noop");
      }
      const mutationId = `curator-mutation:${stored.candidate.candidateId}:${fingerprint(
        now,
      ).slice(0, 10)}`;
      const record: CuratorMutationRecord = {
        mutationId,
        candidateId: stored.candidate.candidateId,
        action: stored.candidate.mutation.type,
        actorId: request.actorId,
        reason: request.reason ?? stored.approval.reason,
        appliedAt: now,
        status: "applied",
        snapshot,
        rollbackRef: `curator-rollback:${mutationId}`,
        changedPaths: changedPaths(management),
        management,
      };
      options.store.recordApplied(record);
      return receipt(request, "applied", {
        auditRef: mutationId,
        summary: `applied ${stored.candidate.summary}`,
      });
    }
  }
}

export async function rollbackCuratorMutation(
  store: MemoryCuratorGovernanceStore,
  mutationId: string,
): Promise<CuratorMutationRecord> {
  const record = store.mutations.get(mutationId);
  if (!record) throw new CuratorExecuteError("curator_mutation_not_found");
  try {
    await restoreSnapshot(record.snapshot);
    store.recordRollback(mutationId, "rolled_back");
    return store.mutations.get(mutationId)!;
  } catch (error) {
    store.recordRollback(mutationId, "rollback_failed");
    throw new CuratorExecuteError(
      error instanceof CuratorExecuteError
        ? error.reasonCode
        : "curator_rollback_failed",
    );
  }
}

export async function curatorSkillSourceRef(
  skillsDir: string,
  slug: string,
): Promise<CuratorCandidateSourceRef> {
  const skill = await loadSkill(skillsDir, slug);
  return {
    kind: "skill",
    ref: slug,
    hash: skillSourceHash(skill.sourcePath, skill.bodyMarkdown),
  };
}

async function runSkillMutation(
  options: CuratorGovernanceExecutorOptions,
  candidate: CuratorMutationCandidate,
  dryRun: boolean,
): Promise<SkillManagementResult> {
  const params = manageParams(candidate, dryRun);
  const result = await skillManageTool({
    skillsDir: options.skillsDir,
    manageMode: "curator",
    curatorApproved: true,
    now: options.now,
  }).execute(`curator:${candidate.candidateId}`, params);
  if (!result.details.ok || !result.details.management) {
    throw new CuratorExecuteError(
      result.details.reasonCode ?? "curator_skill_mutation_failed",
    );
  }
  return result.details.management;
}

function manageParams(
  candidate: CuratorMutationCandidate,
  dryRun: boolean,
): Parameters<ReturnType<typeof skillManageTool>["execute"]>[1] {
  const provenance = `curator:${candidate.candidateId}`;
  switch (candidate.mutation.type) {
    case "skill_patch":
      return {
        action: "patch",
        slug: candidate.mutation.slug,
        content: candidate.mutation.content,
        old_string: candidate.mutation.oldString,
        new_string: candidate.mutation.newString,
        dryRun,
        provenance,
      };
    case "skill_create":
      return {
        action: "create",
        slug: candidate.mutation.slug,
        content: candidate.mutation.content,
        dryRun,
        provenance,
      };
    case "skill_archive":
      return {
        action: "delete",
        slug: candidate.mutation.slug,
        absorbed_into: candidate.mutation.absorbedInto,
        dryRun,
        provenance,
      };
    case "sidecar_write":
      return {
        action: "write_file",
        slug: candidate.mutation.slug,
        file_path: candidate.mutation.filePath,
        file_content: candidate.mutation.fileContent,
        dryRun,
        provenance,
      };
  }
}

async function snapshotBeforeMutation(
  options: CuratorGovernanceExecutorOptions,
  candidate: CuratorMutationCandidate,
): Promise<CuratorSnapshotRef> {
  const timestamp = (options.now?.() ?? new Date())
    .toISOString()
    .replace(/[:.]/g, "-");
  const snapshotId = `${safePathPart(candidate.candidateId)}-${timestamp}`;
  const snapshotDir = join(
    options.snapshotDir ?? join(options.skillsDir, ".curator", "snapshots"),
    snapshotId,
  );
  await mkdir(snapshotDir, { recursive: true });

  const skillPath = join(options.skillsDir, `${candidate.mutation.slug}.md`);
  const sidecarPath = join(options.skillsDir, `${candidate.mutation.slug}.d`);
  const skillExisted = await pathExists(skillPath);
  const sidecarExisted = await pathExists(sidecarPath);
  const snapshot: CuratorSnapshotRef = {
    snapshotId,
    snapshotDir,
    createdAt: timestamp,
    skillPath,
    skillExisted,
  };

  if (skillExisted) {
    snapshot.skillSnapshotPath = join(snapshotDir, "skill.md");
    await cp(skillPath, snapshot.skillSnapshotPath);
  }
  if (sidecarExisted) {
    snapshot.sidecarPath = sidecarPath;
    snapshot.sidecarExisted = true;
    snapshot.sidecarSnapshotPath = join(snapshotDir, "sidecar.d");
    await cp(sidecarPath, snapshot.sidecarSnapshotPath, { recursive: true });
  }
  if (candidate.mutation.type === "sidecar_write") {
    const filePath = safeSidecarFilePath(
      sidecarPath,
      candidate.mutation.filePath,
    );
    const fileExisted = await pathExists(filePath);
    snapshot.filePath = filePath;
    snapshot.fileExisted = fileExisted;
    if (fileExisted) {
      snapshot.fileSnapshotPath = join(snapshotDir, "sidecar-file");
      await cp(filePath, snapshot.fileSnapshotPath);
    }
  }
  await writeFile(
    join(snapshotDir, "snapshot.json"),
    `${JSON.stringify(snapshot, null, 2)}\n`,
    "utf8",
  );
  return snapshot;
}

async function restoreSnapshot(snapshot: CuratorSnapshotRef): Promise<void> {
  if (snapshot.skillExisted && snapshot.skillSnapshotPath) {
    await mkdir(dirname(snapshot.skillPath), { recursive: true });
    await cp(snapshot.skillSnapshotPath, snapshot.skillPath);
  } else {
    await rm(snapshot.skillPath, { force: true });
  }

  if (snapshot.filePath) {
    if (snapshot.fileExisted && snapshot.fileSnapshotPath) {
      await mkdir(dirname(snapshot.filePath), { recursive: true });
      await cp(snapshot.fileSnapshotPath, snapshot.filePath);
    } else {
      await rm(snapshot.filePath, { force: true });
    }
    return;
  }

  if (!snapshot.sidecarPath) return;
  if (snapshot.sidecarExisted && snapshot.sidecarSnapshotPath) {
    await rm(snapshot.sidecarPath, { force: true, recursive: true });
    await cp(snapshot.sidecarSnapshotPath, snapshot.sidecarPath, {
      recursive: true,
    });
  } else {
    await rm(snapshot.sidecarPath, { force: true, recursive: true });
  }
}

async function assertCandidateCurrent(
  skillsDir: string,
  candidate: CuratorMutationCandidate,
): Promise<void> {
  for (const ref of candidate.sourceRefs) {
    if (ref.kind !== "skill" || !ref.hash) continue;
    const current = await curatorSkillSourceRef(skillsDir, ref.ref);
    if (current.hash !== ref.hash) {
      throw new CuratorExecuteError("curator_candidate_stale");
    }
  }
}

function assertNotExpired(
  candidate: CuratorMutationCandidate,
  now: string,
): void {
  if (candidate.expiresAt && candidate.expiresAt <= now) {
    throw new CuratorExecuteError("curator_candidate_expired");
  }
}

function requiredCandidate(
  store: MemoryCuratorGovernanceStore,
  candidateId: string | undefined,
): CuratorStoredCandidate {
  if (!candidateId)
    throw new CuratorExecuteError("curator_candidate_id_required");
  const stored = store.getCandidate(candidateId);
  if (!stored) throw new CuratorExecuteError("curator_candidate_not_found");
  return stored;
}

function assertCandidateNotArchived(stored: CuratorStoredCandidate): void {
  if (stored.lifecycle?.state === "archived") {
    throw new CuratorExecuteError("curator_candidate_archived");
  }
}

function loadGovernanceSnapshot(
  stateFilePath: string,
): CuratorGovernanceStoreSnapshot | undefined {
  if (!existsSync(stateFilePath)) return undefined;
  const parsed = JSON.parse(
    readFileSync(stateFilePath, "utf8"),
  ) as CuratorGovernanceStoreSnapshot;
  return parsed;
}

function writeGovernanceSnapshot(
  stateFilePath: string,
  snapshot: CuratorGovernanceStoreSnapshot,
): void {
  mkdirSync(dirname(stateFilePath), { recursive: true });
  const tempPath = `${stateFilePath}.tmp`;
  writeFileSync(tempPath, `${JSON.stringify(snapshot, null, 2)}\n`, "utf8");
  renameSync(tempPath, stateFilePath);
}

function receipt(
  request: CuratorExecuteRequest,
  status: CuratorExecuteReceipt["status"],
  details: {
    summary: string;
    auditRef?: string;
    observationRef?: string;
  },
): CuratorExecuteReceipt {
  return {
    receiptId: `curator-receipt:${request.action}:${fingerprint(
      request.candidateId ?? "",
      request.scopeType ?? "",
      request.scopeId ?? "",
      details.summary,
    ).slice(0, 12)}`,
    status,
    candidateId: request.candidateId,
    auditRef: details.auditRef,
    observationRef: details.observationRef,
    summary: details.summary,
  };
}

function summarizeManagement(
  prefix: string,
  management: SkillManagementResult,
): string {
  return `${prefix} ${management.action} ${management.slug}; changed=${Boolean(
    management.changed,
  )}`;
}

function changedPaths(management: SkillManagementResult): string[] {
  return [
    management.skillPath,
    management.sidecarPath,
    management.filePath,
    management.archivePath,
    management.sidecarArchivePath,
  ].filter((path): path is string => Boolean(path));
}

async function pathExists(path: string): Promise<boolean> {
  try {
    await stat(path);
    return true;
  } catch {
    return false;
  }
}

function safeSidecarFilePath(
  sidecarPath: string,
  relativePath: string,
): string {
  const normalized = relativePath.replace(/\\/g, "/");
  const topLevel = normalized.split("/")[0];
  if (
    normalized.startsWith("/") ||
    normalized.includes("..") ||
    !["references", "templates", "scripts", "assets"].includes(topLevel)
  ) {
    throw new CuratorExecuteError("curator_invalid_file_path");
  }
  const root = resolve(sidecarPath);
  const target = resolve(root, normalized);
  if (target !== root && target.startsWith(`${root}${sep}`)) {
    return target;
  }
  throw new CuratorExecuteError("curator_invalid_file_path");
}

function safePathPart(value: string): string {
  return value.replace(/[^A-Za-z0-9_.-]/g, "_");
}

function skillSourceHash(sourcePath: string, bodyMarkdown: string): string {
  return fingerprint(sourcePath, bodyMarkdown).slice(0, 16);
}

function fingerprint(...parts: readonly string[]): string {
  const hash = createHash("sha256");
  for (const part of parts) {
    hash.update(part);
    hash.update("\0");
  }
  return hash.digest("hex");
}
