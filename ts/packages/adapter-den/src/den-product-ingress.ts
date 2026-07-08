import type {
  DenDataUpdate,
  EventReceipt,
  ProjectId,
  WorkReference,
  WorkReferenceKind,
} from "@rusty-crew/contracts";
import {
  denProductWorkRef,
  sanitizeRouterMetadataProvenance,
} from "./den-router-metadata.js";

export type DenProductEntityKind =
  | "project"
  | "task"
  | "assignment"
  | "message"
  | "document"
  | "memory"
  | "comment"
  | string;

export type DenProductIngressOperation =
  | "observe"
  | "claim"
  | "complete"
  | "retry"
  | "expire";

export interface DenProductIngressPolicyInput {
  operation: DenProductIngressOperation | string;
  entityKind: DenProductEntityKind;
  entityId: string;
  projectId: ProjectId | string;
}

export interface DenProductIngressPolicyPlan {
  status: "allowed" | "denied";
  operation: DenProductIngressOperation | string;
  reasonCode: string;
  reason: string;
  lifecycleOperation: boolean;
}

export type DenProductIngressPolicyPlanner = (
  input: DenProductIngressPolicyInput,
) => Promise<DenProductIngressPolicyPlan> | DenProductIngressPolicyPlan;

export interface DenProductReferenceInput {
  projectId: ProjectId | string;
  entityKind: DenProductEntityKind;
  entityId: string;
  revision?: string;
  workRefKind?: WorkReferenceKind | string;
  label?: string;
  externalUrl?: string;
  operation?: DenProductIngressOperation;
  provenance?: Record<string, unknown>;
}

export interface DenProductDataIngress {
  injectDenDataUpdate(
    update: DenDataUpdate,
  ): Promise<EventReceipt> | EventReceipt;
}

export type DenProductIngressResult =
  | {
      status: "accepted";
      operation: "observe";
      update: DenDataUpdate;
      receipt: EventReceipt;
      workRef: WorkReference;
      provenance: Record<string, unknown>;
    }
  | {
      status: "denied";
      operation: DenProductIngressOperation | string;
      reasonCode: string;
      reason: string;
      workRef: WorkReference;
      provenance: Record<string, unknown>;
    }
  | {
      status: "degraded";
      operation: "observe";
      reasonCode: "den_product_update_failed";
      message: string;
      update: DenDataUpdate;
      workRef: WorkReference;
      provenance: Record<string, unknown>;
    };

export function toDenProductDataUpdate(
  input: DenProductReferenceInput,
): DenDataUpdate {
  return {
    projectId: input.projectId as ProjectId,
    entityKind: input.entityKind,
    entityId: input.entityId,
    revision: input.revision,
  };
}

export function denProductReferenceWorkRef(
  input: DenProductReferenceInput,
): WorkReference {
  return denProductWorkRef({
    refKind: input.workRefKind ?? productEntityToWorkRefKind(input.entityKind),
    id: input.entityId,
    projectId: input.projectId,
    label: input.label,
    externalUrl: input.externalUrl,
  });
}

export async function ingestDenProductReference(
  input: DenProductReferenceInput,
  ingress: DenProductDataIngress,
  policyPlanner: DenProductIngressPolicyPlanner = planDenProductIngressPolicy,
): Promise<DenProductIngressResult> {
  const operation = input.operation ?? "observe";
  const workRef = denProductReferenceWorkRef(input);
  const provenance = sanitizeRouterMetadataProvenance(input.provenance ?? {});
  const policy = await policyPlanner({
    operation,
    entityKind: input.entityKind,
    entityId: input.entityId,
    projectId: input.projectId,
  });

  if (policy.status === "denied") {
    return {
      status: "denied",
      operation: policy.operation,
      reasonCode: policy.reasonCode,
      reason: policy.reason,
      workRef,
      provenance,
    };
  }
  const observeOperation = "observe" as const;

  const update = toDenProductDataUpdate(input);
  try {
    const receipt = await ingress.injectDenDataUpdate(update);
    return {
      status: "accepted",
      operation: observeOperation,
      update,
      receipt,
      workRef,
      provenance,
    };
  } catch (error) {
    return {
      status: "degraded",
      operation: observeOperation,
      reasonCode: "den_product_update_failed",
      message: error instanceof Error ? error.message : String(error),
      update,
      workRef,
      provenance,
    };
  }
}

export function planDenProductIngressPolicy(
  input: DenProductIngressPolicyInput,
): DenProductIngressPolicyPlan {
  const operation = input.operation.trim() || "observe";
  const lifecycleOperation = operation !== "observe";
  if (lifecycleOperation) {
    return {
      status: "denied",
      operation,
      reasonCode: "adapter_lifecycle_operation_denied",
      reason:
        "Den product ingress may observe/reference Den data but cannot mutate Crew lifecycle state.",
      lifecycleOperation,
    };
  }
  return {
    status: "allowed",
    operation,
    reasonCode: "den_product_observe_allowed",
    reason: "Den product ingress observation/reference update is allowed.",
    lifecycleOperation,
  };
}

function productEntityToWorkRefKind(
  entityKind: DenProductEntityKind,
): WorkReferenceKind | string {
  switch (entityKind) {
    case "project":
    case "task":
    case "assignment":
    case "run":
    case "channel_message":
      return entityKind;
    case "message":
      return "channel_message";
    default:
      return entityKind;
  }
}
