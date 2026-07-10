import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  writeFileSync,
} from "node:fs";
import { cp, mkdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import { dirname, join, relative, resolve, sep } from "node:path";
import type {
  CuratorExecuteReceipt,
  CuratorExecuteRequest,
} from "./planning-tools.js";
import { CuratorExecuteError } from "./planning-tools.js";
import { loadSkill } from "./profile-loading.js";
import { skillManageTool, type SkillManagementResult } from "./skills-tools.js";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import type {
  CuratorCandidate,
  CuratorCandidateBatch,
  CuratorCandidateSourceRef,
} from "./curator-candidates.js";
import type { CuratorActivityReceipt } from "./curator-observation.js";

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
  planner?: CuratorGovernancePlanner;
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

export interface PersistableCuratorGovernanceStore {
  persist(): void | Promise<void>;
}

export type RustCuratorGovernanceAction =
  | "preview_candidate"
  | "approve_candidate"
  | "apply_candidate";

export type RustCuratorStoredCandidateStatus =
  | "proposed"
  | "previewed"
  | "approved"
  | "applied";

export type RustCuratorLifecycleState = "active" | "stale" | "archived";

export interface RustCuratorGovernancePlanInput {
  action: RustCuratorGovernanceAction;
  candidate: {
    candidate_id: string;
    fingerprint: string;
    status: RustCuratorStoredCandidateStatus;
    lifecycle_state?: RustCuratorLifecycleState;
    lifecycle_reason_code?: string;
    expires_at?: string;
    approval_fingerprint?: string;
    source_current: boolean;
    source_current_reason_code?: string;
  };
  now: string;
  actor?: string;
  reason?: string;
  dry_run: boolean;
}

export interface RustCuratorGovernancePlan {
  accepted: boolean;
  action: RustCuratorGovernanceAction;
  candidate_id: string;
  audit_ref?: string;
  receipt_id: string;
  resulting_status?: RustCuratorStoredCandidateStatus;
  diagnostics: readonly {
    reason_code: string;
    message: string;
  }[];
}

export type CuratorGovernancePlanner = (
  input: RustCuratorGovernancePlanInput,
) => Promise<RustCuratorGovernancePlan>;

interface CandidateCurrentness {
  sourceCurrent: boolean;
  reasonCode?: string;
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

  persist(): void {
    writeGovernanceSnapshot(this.stateFilePath, this.snapshot());
  }
}

export class NativeCuratorGovernanceStore
  extends MemoryCuratorGovernanceStore
  implements PersistableCuratorGovernanceStore
{
  static async load(input: {
    bridge: CuratorPersistenceBridge;
    now: string;
    skillsDir: string;
    snapshotRoot: string;
    publishActivity?: (receipt: CuratorActivityReceipt) => Promise<void>;
  }): Promise<NativeCuratorGovernanceStore> {
    const store = new NativeCuratorGovernanceStore({
      bridge: input.bridge,
      now: () => input.now,
      skillsDir: input.skillsDir,
      snapshotRoot: input.snapshotRoot,
      publishActivity: input.publishActivity,
    });
    for (const record of await listAllCuratorCandidates(input.bridge)) {
      const payload = record.candidatePayload as NativeCandidatePayload;
      store.candidates.set(record.candidateId, payload.stored);
      store.candidateRevisions.set(record.candidateId, record.revision);
      store.candidateHashes.set(record.candidateId, stableHash(payload.stored));
      store.candidateProfileIds.set(record.candidateId, payload.profileId);
      if (payload.stored.approval) {
        store.persistedApprovalIds.add(approvalId(payload.stored.approval));
      }
    }
    for (const record of await listAllCuratorMutations(input.bridge)) {
      const mutation = mutationFromNativeRecord(
        record,
        input.skillsDir,
        input.snapshotRoot,
      );
      store.mutations.set(record.mutationId, mutation);
      store.mutationRevisions.set(record.mutationId, record.revision);
      store.mutationHashes.set(record.mutationId, stableHash(mutation));
    }
    return store;
  }

  private readonly bridge: CuratorPersistenceBridge;
  private readonly now: () => string;
  private readonly skillsDir: string;
  private readonly snapshotRoot: string;
  private readonly publishActivityCallback:
    | ((receipt: CuratorActivityReceipt) => Promise<void>)
    | undefined;
  private readonly candidateRevisions = new Map<string, number>();
  private readonly candidateHashes = new Map<string, string>();
  private readonly mutationRevisions = new Map<string, number>();
  private readonly mutationHashes = new Map<string, string>();
  private readonly persistedApprovalIds = new Set<string>();
  private readonly candidateProfileIds = new Map<string, string>();
  readonly activityProjectionFailures: Array<{
    receiptId: string;
    message: string;
  }> = [];
  lastActivityReceipt?: CuratorActivityReceipt;

  private constructor(input: {
    bridge: CuratorPersistenceBridge;
    now: () => string;
    skillsDir: string;
    snapshotRoot: string;
    publishActivity?: (receipt: CuratorActivityReceipt) => Promise<void>;
  }) {
    super();
    this.bridge = input.bridge;
    this.now = input.now;
    this.skillsDir = resolve(input.skillsDir);
    this.snapshotRoot = resolve(input.snapshotRoot);
    this.publishActivityCallback = input.publishActivity;
  }

  async persist(): Promise<void> {
    const persistedCandidates = new Set<string>();
    for (const mutation of this.mutations.values()) {
      if (
        this.mutationHashes.get(mutation.mutationId) === stableHash(mutation)
      ) {
        continue;
      }
      const stored = requiredCandidate(this, mutation.candidateId);
      const candidateChanged =
        this.candidateHashes.get(mutation.candidateId) !== stableHash(stored);
      const result = (await this.bridge.applyCuratorGovernanceWrite({
        candidate: candidateChanged ? this.candidateWrite(stored) : undefined,
        approval: this.newApproval(stored),
        snapshot:
          this.mutationRevisions.get(mutation.mutationId) === undefined
            ? snapshotRecord(mutation, this.skillsDir, this.snapshotRoot)
            : undefined,
        mutation: this.mutationWrite(mutation),
        receipt: auditReceipt(
          mutationActivityKind(mutation.status),
          mutation.mutationId,
          stored,
          this.now(),
          this.profileIdForCandidate(stored.candidate),
          mutation.actorId,
          mutation.error,
        ),
      })) as NativeGovernanceWriteResult;
      if (result.candidate) {
        this.rememberCandidate(stored, result.candidate.revision);
        persistedCandidates.add(stored.candidate.candidateId);
      }
      if (result.mutation) {
        this.mutationRevisions.set(
          mutation.mutationId,
          result.mutation.revision,
        );
        this.mutationHashes.set(mutation.mutationId, stableHash(mutation));
      }
      await this.publishActivity(result.receipt);
    }

    for (const stored of this.candidates.values()) {
      const candidateId = stored.candidate.candidateId;
      if (
        persistedCandidates.has(candidateId) ||
        this.candidateHashes.get(candidateId) === stableHash(stored)
      ) {
        continue;
      }
      const result = (await this.bridge.applyCuratorGovernanceWrite({
        candidate: this.candidateWrite(stored),
        approval: this.newApproval(stored),
        receipt: auditReceipt(
          candidateActivityKind(stored),
          candidateId,
          stored,
          this.now(),
          this.profileIdForCandidate(stored.candidate),
          stored.approval?.actorId,
          stored.lifecycle?.reasonCode,
        ),
      })) as NativeGovernanceWriteResult;
      if (!result.candidate) {
        throw new CuratorExecuteError("curator_candidate_persist_failed");
      }
      this.rememberCandidate(stored, result.candidate.revision);
      await this.publishActivity(result.receipt);
    }
  }

  async recordRejectedRequest(
    request: CuratorExecuteRequest,
    error: unknown,
  ): Promise<void> {
    const stored = request.candidateId
      ? this.candidates.get(request.candidateId)
      : undefined;
    const reasonCode =
      error instanceof CuratorExecuteError
        ? error.reasonCode
        : "curator_execution_failed";
    const activityKind =
      request.action === "apply_candidate"
        ? "mutation_failed"
        : "candidate_denied";
    const subjectId =
      request.candidateId ?? request.scopeId ?? request.scopeType ?? "service";
    const profileId = stored
      ? this.profileIdForCandidate(stored.candidate)
      : request.scopeType === "profile" && request.scopeId
        ? request.scopeId
        : "service";
    try {
      const result = (await this.bridge.applyCuratorGovernanceWrite({
        receipt: failureAuditReceipt({
          activityKind,
          subjectId,
          candidateId: request.candidateId,
          correlationId: stored
            ? `curator:${stored.candidate.batchId}`
            : `curator:${profileId}`,
          profileId,
          actorId: request.actorId,
          reasonCode,
          now: this.now(),
        }),
      })) as NativeGovernanceWriteResult;
      await this.publishActivity(result.receipt);
    } catch (recordError) {
      this.activityProjectionFailures.push({
        receiptId: `curator-unrecorded:${activityKind}:${subjectId}`,
        message: `failed to record curator failure: ${recordError instanceof Error ? recordError.message : String(recordError)}`,
      });
    }
  }

  private candidateWrite(stored: CuratorStoredCandidate): unknown {
    const candidateId = stored.candidate.candidateId;
    const profileId = this.profileIdForCandidate(stored.candidate);
    return {
      record: {
        candidateId,
        batchId: stored.candidate.batchId,
        profileId,
        sessionId: undefined,
        kind: stored.candidate.kind,
        summary: stored.candidate.summary,
        fingerprint: stored.candidate.fingerprint,
        candidatePayload: {
          stored,
          profileId,
        } satisfies NativeCandidatePayload,
        mutation: stored.candidate.mutation,
        sourceRefs: stored.candidate.sourceRefs,
        expiresAt: stored.candidate.expiresAt,
        status: stored.status,
        lifecycleState: stored.lifecycle?.state ?? "active",
        lifecycleReasonCode: stored.lifecycle?.reasonCode,
        revision: this.candidateRevisions.get(candidateId) ?? 0,
        createdAt: candidateCreatedAt(stored, this.now()),
        updatedAt: this.now(),
      },
      expected_revision: this.candidateRevisions.get(candidateId),
    };
  }

  private mutationWrite(record: CuratorMutationRecord): unknown {
    return {
      record: {
        mutationId: record.mutationId,
        receiptId: `curator-mutation:${record.mutationId}`,
        candidateId: record.candidateId,
        candidateRevision: this.candidateRevisions.get(record.candidateId) ?? 0,
        action: record.action,
        actorId: record.actorId,
        reason: record.reason,
        snapshotId: record.snapshot.snapshotId,
        mutationPayload: {
          management: record.management,
          error: record.error,
          snapshotManifest: snapshotManifest(
            record.snapshot,
            this.skillsDir,
            this.snapshotRoot,
          ),
        },
        changedPaths: record.changedPaths,
        management: record.management,
        status: record.status,
        errorReasonCode: record.error,
        revision: this.mutationRevisions.get(record.mutationId) ?? 0,
        createdAt: record.appliedAt,
        appliedAt: record.status === "applied" ? record.appliedAt : undefined,
        rolledBackAt: record.status === "rolled_back" ? this.now() : undefined,
      },
      expected_revision: this.mutationRevisions.get(record.mutationId),
    };
  }

  private newApproval(
    stored: CuratorStoredCandidate,
  ): NativeApprovalRecord | undefined {
    if (!stored.approval) return undefined;
    const id = approvalId(stored.approval);
    if (this.persistedApprovalIds.has(id)) return undefined;
    return {
      approvalId: id,
      receiptId: `curator-approval:${id}`,
      candidateId: stored.approval.candidateId,
      candidateRevision:
        this.candidateRevisions.get(stored.approval.candidateId) ?? 0,
      fingerprint: stored.approval.fingerprint,
      actorId: stored.approval.actorId,
      reason: stored.approval.reason,
      approvedAt: stored.approval.approvedAt,
      supersededAt: undefined,
    };
  }

  private rememberCandidate(
    stored: CuratorStoredCandidate,
    revision: number,
  ): void {
    this.candidateRevisions.set(stored.candidate.candidateId, revision);
    this.candidateHashes.set(stored.candidate.candidateId, stableHash(stored));
    if (stored.approval) {
      this.persistedApprovalIds.add(approvalId(stored.approval));
    }
  }

  private profileIdForCandidate(candidate: CuratorMutationCandidate): string {
    const profileId = String(
      this.candidateProfileIds.get(candidate.candidateId) ??
        this.batches.get(candidate.batchId)?.profileId ??
        "service",
    );
    this.candidateProfileIds.set(candidate.candidateId, profileId);
    return profileId;
  }

  private async publishActivity(
    receipt: CuratorActivityReceipt,
  ): Promise<void> {
    this.lastActivityReceipt = receipt;
    if (!this.publishActivityCallback) return;
    try {
      await this.publishActivityCallback(receipt);
    } catch (error) {
      this.activityProjectionFailures.push({
        receiptId: receipt.receiptId,
        message: error instanceof Error ? error.message : String(error),
      });
      if (this.activityProjectionFailures.length > 100) {
        this.activityProjectionFailures.shift();
      }
    }
  }
}

type CuratorPersistenceBridge = Pick<
  NativeBridgeModule,
  | "applyCuratorGovernanceWrite"
  | "listCuratorCandidates"
  | "listCuratorMutations"
>;

interface NativeCandidatePayload {
  stored: CuratorStoredCandidate;
  profileId: string;
}

interface NativeCandidateRecord {
  candidateId: string;
  candidatePayload: unknown;
  revision: number;
}

interface NativeMutationRecord {
  mutationId: string;
  candidateId: string;
  action: CuratorMutationRecord["action"];
  actorId?: string;
  reason: string;
  snapshotId: string;
  mutationPayload?: {
    management?: SkillManagementResult;
    error?: string;
    snapshotManifest?: Record<string, unknown>;
  };
  changedPaths: string[];
  status: CuratorMutationStatus;
  revision: number;
  createdAt: string;
  appliedAt?: string;
}

interface NativePage<T> {
  items: T[];
  next_offset?: number | null;
}

interface NativeGovernanceWriteResult {
  candidate?: NativeCandidateRecord;
  mutation?: NativeMutationRecord;
  receipt: CuratorActivityReceipt;
}

interface NativeApprovalRecord {
  approvalId: string;
  receiptId: string;
  candidateId: string;
  candidateRevision: number;
  fingerprint: string;
  actorId?: string;
  reason: string;
  approvedAt: string;
  supersededAt?: string;
}

async function listAllCuratorCandidates(
  bridge: CuratorPersistenceBridge,
): Promise<NativeCandidateRecord[]> {
  return listAllNativePages(
    (offset) =>
      bridge.listCuratorCandidates({
        page: { limit: 200, offset },
      }) as Promise<NativePage<NativeCandidateRecord>>,
  );
}

async function listAllCuratorMutations(
  bridge: CuratorPersistenceBridge,
): Promise<NativeMutationRecord[]> {
  return listAllNativePages(
    (offset) =>
      bridge.listCuratorMutations({
        page: { limit: 200, offset },
      }) as Promise<NativePage<NativeMutationRecord>>,
  );
}

async function listAllNativePages<T>(
  read: (offset: number) => Promise<NativePage<T>>,
): Promise<T[]> {
  const items: T[] = [];
  let offset = 0;
  for (;;) {
    const page = await read(offset);
    items.push(...page.items);
    if (page.next_offset == null) return items;
    offset = page.next_offset;
  }
}

function stableHash(value: unknown): string {
  return createHash("sha256").update(JSON.stringify(value)).digest("hex");
}

function approvalId(approval: CuratorApprovalRecord): string {
  return `${approval.candidateId}:${approval.fingerprint}`;
}

function candidateCreatedAt(
  stored: CuratorStoredCandidate,
  fallback: string,
): string {
  return (
    stored.previewedAt ??
    stored.approval?.approvedAt ??
    stored.lifecycle?.lastTransitionAt ??
    fallback
  );
}

function candidateActivityKind(stored: CuratorStoredCandidate): string {
  if (stored.lifecycle?.state === "stale") return "candidate_staled";
  if (stored.lifecycle?.state === "archived") return "candidate_archived";
  switch (stored.status) {
    case "previewed":
      return "candidate_previewed";
    case "approved":
      return "candidate_approved";
    case "applied":
      return "mutation_applied";
    case "proposed":
      return "candidate_discovered";
  }
}

function mutationActivityKind(status: CuratorMutationStatus): string {
  switch (status) {
    case "applied":
      return "mutation_applied";
    case "failed":
      return "mutation_failed";
    case "rolled_back":
      return "rollback_completed";
    case "rollback_failed":
      return "rollback_failed";
  }
}

function auditReceipt(
  activityKind: string,
  subjectId: string,
  stored: CuratorStoredCandidate,
  now: string,
  profileId: string,
  actorId?: string,
  reasonCode?: string,
): unknown {
  const idempotencyKey = stableHash({
    activityKind,
    subjectId,
    stored,
  });
  return {
    sequence: 0,
    receiptId: `curator-receipt:${activityKind}:${idempotencyKey.slice(0, 20)}`,
    correlationId: `curator:${stored.candidate.batchId}`,
    idempotencyKey,
    profileId,
    sessionId: undefined,
    candidateId: stored.candidate.candidateId,
    mutationId:
      activityKind.startsWith("mutation_") ||
      activityKind.startsWith("rollback_")
        ? subjectId
        : undefined,
    activityKind,
    outcome: activityKind.endsWith("failed") ? "failed" : "accepted",
    reasonCode,
    summary: `${activityKind} for ${subjectId}`,
    actorId,
    details: undefined,
    occurredAt: now,
  };
}

function failureAuditReceipt(input: {
  activityKind: string;
  subjectId: string;
  candidateId?: string;
  correlationId: string;
  profileId: string;
  actorId?: string;
  reasonCode: string;
  now: string;
}): unknown {
  const idempotencyKey = stableHash(input);
  return {
    sequence: 0,
    receiptId: `curator-receipt:${input.activityKind}:${idempotencyKey.slice(0, 20)}`,
    correlationId: input.correlationId,
    idempotencyKey,
    profileId: input.profileId,
    candidateId: input.candidateId,
    mutationId:
      input.activityKind === "mutation_failed" ? input.subjectId : undefined,
    activityKind: input.activityKind,
    outcome: "failed",
    reasonCode: input.reasonCode,
    summary: `${input.activityKind} for ${input.subjectId}: ${input.reasonCode}`,
    actorId: input.actorId,
    occurredAt: input.now,
  };
}

function snapshotRecord(
  mutation: CuratorMutationRecord,
  skillsDir: string,
  snapshotRoot: string,
): unknown {
  return {
    snapshotId: mutation.snapshot.snapshotId,
    candidateId: mutation.candidateId,
    snapshotRootRef: safeRelative(snapshotRoot, mutation.snapshot.snapshotDir),
    manifest: snapshotManifest(mutation.snapshot, skillsDir, snapshotRoot),
    status: "consumed",
    createdAt: mutation.snapshot.createdAt,
    verifiedAt: mutation.appliedAt,
  };
}

function snapshotManifest(
  snapshot: CuratorSnapshotRef,
  skillsDir: string,
  snapshotRoot: string,
): Record<string, unknown> {
  return {
    snapshotDirRef: safeRelative(snapshotRoot, snapshot.snapshotDir),
    skillPathRef: safeRelative(skillsDir, snapshot.skillPath),
    skillExisted: snapshot.skillExisted,
    skillSnapshotPathRef: snapshot.skillSnapshotPath
      ? safeRelative(snapshotRoot, snapshot.skillSnapshotPath)
      : undefined,
    sidecarPathRef: snapshot.sidecarPath
      ? safeRelative(skillsDir, snapshot.sidecarPath)
      : undefined,
    sidecarExisted: snapshot.sidecarExisted,
    sidecarSnapshotPathRef: snapshot.sidecarSnapshotPath
      ? safeRelative(snapshotRoot, snapshot.sidecarSnapshotPath)
      : undefined,
    filePathRef: snapshot.filePath
      ? safeRelative(skillsDir, snapshot.filePath)
      : undefined,
    fileExisted: snapshot.fileExisted,
    fileSnapshotPathRef: snapshot.fileSnapshotPath
      ? safeRelative(snapshotRoot, snapshot.fileSnapshotPath)
      : undefined,
  };
}

function mutationFromNativeRecord(
  record: NativeMutationRecord,
  skillsDir: string,
  snapshotRoot: string,
): CuratorMutationRecord {
  const payload = record.mutationPayload ?? {};
  const manifest = payload.snapshotManifest;
  if (!manifest) {
    throw new CuratorExecuteError("curator_snapshot_unavailable");
  }
  return {
    mutationId: record.mutationId,
    candidateId: record.candidateId,
    action: record.action,
    actorId: record.actorId,
    reason: record.reason,
    appliedAt: record.appliedAt ?? record.createdAt,
    status: record.status,
    snapshot: snapshotFromManifest(
      record.snapshotId,
      manifest,
      skillsDir,
      snapshotRoot,
      record.createdAt,
    ),
    rollbackRef: `curator-rollback:${record.mutationId}`,
    changedPaths: record.changedPaths,
    management: payload.management,
    error: payload.error,
  };
}

function snapshotFromManifest(
  snapshotId: string,
  manifest: Record<string, unknown>,
  skillsDir: string,
  snapshotRoot: string,
  createdAt: string,
): CuratorSnapshotRef {
  const ref = (name: string): string | undefined =>
    typeof manifest[name] === "string" ? String(manifest[name]) : undefined;
  const rooted = (root: string, name: string): string | undefined => {
    const value = ref(name);
    return value ? resolve(root, value) : undefined;
  };
  return {
    snapshotId,
    snapshotDir: rooted(snapshotRoot, "snapshotDirRef") ?? snapshotRoot,
    createdAt,
    skillPath: rooted(skillsDir, "skillPathRef") ?? skillsDir,
    skillExisted: Boolean(manifest.skillExisted),
    skillSnapshotPath: rooted(snapshotRoot, "skillSnapshotPathRef"),
    sidecarPath: rooted(skillsDir, "sidecarPathRef"),
    sidecarExisted: Boolean(manifest.sidecarExisted),
    sidecarSnapshotPath: rooted(snapshotRoot, "sidecarSnapshotPathRef"),
    filePath: rooted(skillsDir, "filePathRef"),
    fileExisted: Boolean(manifest.fileExisted),
    fileSnapshotPath: rooted(snapshotRoot, "fileSnapshotPathRef"),
  };
}

function safeRelative(root: string, target: string): string {
  const ref = relative(resolve(root), resolve(target));
  if (!ref || ref === "." || ref.startsWith(`..${sep}`) || ref === "..") {
    if (ref === ".") return "snapshot";
    throw new CuratorExecuteError("curator_snapshot_ref_invalid");
  }
  return ref;
}

export function createCuratorGovernanceExecutor(
  options: CuratorGovernanceExecutorOptions,
): (request: CuratorExecuteRequest) => Promise<CuratorExecuteReceipt> {
  return async (request) => {
    try {
      return await executeCuratorGovernanceRequest(options, request);
    } catch (error) {
      if (options.store instanceof NativeCuratorGovernanceStore) {
        await options.store.recordRejectedRequest(request, error);
      }
      throw error;
    }
  };
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
      await persistCuratorStore(options.store);
      return receipt(request, "requested", {
        auditRef: batch.reportId,
        summary: `scan produced ${batch.candidateCount} candidate(s)`,
      });
    }
    case "preview_candidate": {
      const stored = requiredCandidate(options.store, request.candidateId);
      const currentness = await candidateSourcesCurrent(
        options.skillsDir,
        stored.candidate,
      );
      const plan = await planCuratorGovernanceTransition(
        options,
        request,
        stored,
        "preview_candidate",
        now,
        currentness,
      );
      const management = await runSkillMutation(
        options,
        stored.candidate,
        true,
      );
      options.store.recordPreview(stored.candidate.candidateId, now);
      await persistCuratorStore(options.store);
      return receipt(request, "previewed", {
        receiptId: plan.receipt_id,
        auditRef: plan.audit_ref,
        summary: summarizeManagement("preview", management),
      });
    }
    case "approve_candidate": {
      const stored = requiredCandidate(options.store, request.candidateId);
      const currentness = await candidateSourcesCurrent(
        options.skillsDir,
        stored.candidate,
      );
      const plan = await planCuratorGovernanceTransition(
        options,
        request,
        stored,
        "approve_candidate",
        now,
        currentness,
      );
      options.store.approve(stored.candidate.candidateId, {
        candidateId: stored.candidate.candidateId,
        actorId: request.actorId,
        reason: request.reason ?? "curator approval",
        approvedAt: now,
        fingerprint: stored.candidate.fingerprint,
      });
      await persistCuratorStore(options.store);
      return receipt(request, "approved", {
        receiptId: plan.receipt_id,
        auditRef: plan.audit_ref,
        summary: `approved ${stored.candidate.summary}`,
      });
    }
    case "apply_candidate": {
      const stored = requiredCandidate(options.store, request.candidateId);
      const currentness = await candidateSourcesCurrent(
        options.skillsDir,
        stored.candidate,
      );
      const plan = await planCuratorGovernanceTransition(
        options,
        request,
        stored,
        "apply_candidate",
        now,
        currentness,
      );
      if (request.dryRun) {
        const management = await runSkillMutation(
          options,
          stored.candidate,
          true,
        );
        return receipt(request, "previewed", {
          receiptId: plan.receipt_id,
          auditRef: plan.audit_ref,
          summary: summarizeManagement("dry-run apply", management),
        });
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
        reason: request.reason ?? stored.approval?.reason ?? "curator apply",
        appliedAt: now,
        status: "applied",
        snapshot,
        rollbackRef: `curator-rollback:${mutationId}`,
        changedPaths: changedPaths(management),
        management,
      };
      options.store.recordApplied(record);
      await persistCuratorStore(options.store);
      return receipt(request, "applied", {
        receiptId: plan.receipt_id,
        auditRef: plan.audit_ref ?? mutationId,
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
    await persistCuratorStore(store);
    return store.mutations.get(mutationId)!;
  } catch (error) {
    store.recordRollback(mutationId, "rollback_failed");
    await persistCuratorStore(store);
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

async function planCuratorGovernanceTransition(
  options: CuratorGovernanceExecutorOptions,
  request: CuratorExecuteRequest,
  stored: CuratorStoredCandidate,
  action: RustCuratorGovernanceAction,
  now: string,
  currentness: CandidateCurrentness,
): Promise<RustCuratorGovernancePlan> {
  if (!options.planner) {
    throw new CuratorExecuteError("curator_governance_planner_unavailable");
  }
  const plan = await options.planner({
    action,
    candidate: curatorGovernanceCandidateSnapshot(stored, currentness),
    now,
    ...(request.actorId ? { actor: request.actorId } : {}),
    ...(request.reason ? { reason: request.reason } : {}),
    dry_run: Boolean(request.dryRun),
  });
  if (!plan.accepted) {
    throw new CuratorExecuteError(
      plan.diagnostics[0]?.reason_code ?? "curator_governance_rejected",
    );
  }
  return plan;
}

function curatorGovernanceCandidateSnapshot(
  stored: CuratorStoredCandidate,
  currentness: CandidateCurrentness,
): RustCuratorGovernancePlanInput["candidate"] {
  return {
    candidate_id: stored.candidate.candidateId,
    fingerprint: stored.candidate.fingerprint,
    status: curatorStoredStatus(stored.status),
    ...(stored.lifecycle?.state
      ? { lifecycle_state: stored.lifecycle.state }
      : {}),
    ...(stored.lifecycle?.reasonCode
      ? { lifecycle_reason_code: stored.lifecycle.reasonCode }
      : {}),
    ...(stored.candidate.expiresAt
      ? { expires_at: stored.candidate.expiresAt }
      : {}),
    ...(stored.approval?.fingerprint
      ? { approval_fingerprint: stored.approval.fingerprint }
      : {}),
    source_current: currentness.sourceCurrent,
    ...(currentness.reasonCode
      ? { source_current_reason_code: currentness.reasonCode }
      : {}),
  };
}

function curatorStoredStatus(
  status: CuratorStoredCandidateStatus,
): RustCuratorStoredCandidateStatus {
  if (status === "proposed") return "proposed";
  if (status === "previewed") return "previewed";
  if (status === "approved") return "approved";
  if (status === "applied") return "applied";
  return "proposed";
}

async function candidateSourcesCurrent(
  skillsDir: string,
  candidate: CuratorMutationCandidate,
): Promise<CandidateCurrentness> {
  for (const ref of candidate.sourceRefs) {
    if (ref.kind !== "skill" || !ref.hash) continue;
    const current = await curatorSkillSourceRef(skillsDir, ref.ref);
    if (current.hash !== ref.hash) {
      return { sourceCurrent: false, reasonCode: "curator_candidate_stale" };
    }
  }
  return { sourceCurrent: true };
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

async function persistCuratorStore(
  store: MemoryCuratorGovernanceStore,
): Promise<void> {
  const persist = (store as Partial<PersistableCuratorGovernanceStore>).persist;
  if (persist) {
    await persist.call(store);
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
    receiptId?: string;
    summary: string;
    auditRef?: string;
    observationRef?: string;
  },
): CuratorExecuteReceipt {
  return {
    receiptId:
      details.receiptId ??
      `curator-receipt:${request.action}:${fingerprint(
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
