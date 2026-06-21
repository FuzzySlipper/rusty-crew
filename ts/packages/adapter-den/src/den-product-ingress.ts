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
      operation: Exclude<DenProductIngressOperation, "observe">;
      reasonCode: "adapter_lifecycle_operation_denied";
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
): Promise<DenProductIngressResult> {
  const operation = input.operation ?? "observe";
  const workRef = denProductReferenceWorkRef(input);
  const provenance = sanitizeRouterMetadataProvenance(input.provenance ?? {});

  if (operation !== "observe") {
    return {
      status: "denied",
      operation,
      reasonCode: "adapter_lifecycle_operation_denied",
      workRef,
      provenance,
    };
  }

  const update = toDenProductDataUpdate(input);
  try {
    const receipt = await ingress.injectDenDataUpdate(update);
    return {
      status: "accepted",
      operation,
      update,
      receipt,
      workRef,
      provenance,
    };
  } catch (error) {
    return {
      status: "degraded",
      operation,
      reasonCode: "den_product_update_failed",
      message: error instanceof Error ? error.message : String(error),
      update,
      workRef,
      provenance,
    };
  }
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
