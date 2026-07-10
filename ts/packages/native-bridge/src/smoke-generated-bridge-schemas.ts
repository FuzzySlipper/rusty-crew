import assert from "node:assert/strict";

import { Value } from "typebox/value";

import { BridgeValidationError } from "./bridge-validation.js";
import { withGeneratedBridgeOutputValidation } from "./generated-binding-validation.js";
import {
  bridgeWireSchemaArtifact,
  generatedBridgeOutputSchemas,
  type GeneratedBridgeOutputOperation,
} from "./generated/bridge-wire-schemas.js";

for (const [operation, samples] of Object.entries(
  bridgeWireSchemaArtifact.sampleOutputs,
) as Array<[GeneratedBridgeOutputOperation, readonly unknown[]]>) {
  for (const sample of samples) {
    assert(
      Value.Check(generatedBridgeOutputSchemas[operation], sample),
      `Rust sample must satisfy generated ${operation} schema`,
    );
  }
}

const invalidScope = structuredClone(
  bridgeWireSchemaArtifact.sampleOutputs.save_data_bank_scope[0],
) as unknown as { status: string };
invalidScope.status = "invalid_status";
assert(
  !Value.Check(generatedBridgeOutputSchemas.save_data_bank_scope, invalidScope),
  "generated enum schema must reject unknown data-bank status",
);

const invalidJump = structuredClone(
  bridgeWireSchemaArtifact.sampleOutputs.resolve_conversation_jump[0],
) as unknown as { target: unknown };
invalidJump.target = { type: "unknown_target" };
assert(
  !Value.Check(
    generatedBridgeOutputSchemas.resolve_conversation_jump,
    invalidJump,
  ),
  "generated tagged-union schema must reject unknown jump target",
);

const invalidAttachment = structuredClone(
  bridgeWireSchemaArtifact.sampleOutputs.query_attachments[0],
) as unknown as Array<{ links: Array<{ link_id?: string }> }>;
delete invalidAttachment[0]?.links[0]?.link_id;
assert(
  !Value.Check(
    generatedBridgeOutputSchemas.query_attachments,
    invalidAttachment,
  ),
  "generated nested schema must reject missing attachment-link identity",
);

const validSlot =
  bridgeWireSchemaArtifact.sampleOutputs.create_chat_message_slot[0];
const validBinding = withGeneratedBridgeOutputValidation(
  {
    createChatMessageSlotJson: () => JSON.stringify(validSlot),
  },
  { RUSTY_CREW_BRIDGE_VALIDATE: "1" },
);
assert.deepEqual(
  JSON.parse(validBinding.createChatMessageSlotJson()),
  validSlot,
);

const invalidBinding = withGeneratedBridgeOutputValidation(
  {
    createChatMessageSlotJson: () => JSON.stringify({ status: "created" }),
  },
  { RUSTY_CREW_BRIDGE_VALIDATE: "1" },
);
assert.throws(
  () => invalidBinding.createChatMessageSlotJson(),
  BridgeValidationError,
  "raw binding chokepoint must reject invalid generated-schema output",
);

const disabledRawBinding = {
  createChatMessageSlotJson: () => JSON.stringify({ status: "created" }),
};
assert.equal(
  withGeneratedBridgeOutputValidation(disabledRawBinding, {
    RUSTY_CREW_BRIDGE_VALIDATE: "0",
  }),
  disabledRawBinding,
  "disabled bridge validation must preserve the production fast path",
);

console.log(
  JSON.stringify({
    generatedSchemas: Object.keys(generatedBridgeOutputSchemas).length,
    rustSampleOperations: Object.keys(bridgeWireSchemaArtifact.sampleOutputs)
      .length,
    nestedEnumTagAndNullCoverage: true,
    rawBindingChokepoint: true,
  }),
);
