import type { BrainWakeProviderStateInput } from "@rusty-crew/contracts";

import type {
  NativeBrainWakeProviderStateInput,
  OpenAiResponsesBrainRunInput,
} from "./public-api.js";
import { toNativeBodyState } from "./event-body-wire.js";

export function toNativeOpenAiResponsesBrainRunInput(
  input: OpenAiResponsesBrainRunInput,
): unknown {
  return {
    wakeId: input.wakeId,
    sessionId: input.sessionId,
    bodyState: toNativeBodyState(input.bodyState),
    tools: input.tools?.map((tool) => ({
      name: tool.name,
      description: tool.description,
      inputSchema: tool.inputSchema,
    })),
    providerState: input.providerState
      ? toNativeProviderStateInput(input.providerState)
      : undefined,
    providerStateAbsence: input.providerStateAbsence,
    continuationState: input.continuationState,
    config: input.config,
    client:
      input.client?.mode === "live"
        ? {
            mode: "live",
            base_url: input.client.baseUrl,
            api_key: input.client.apiKey,
            auth_kind: input.client.authKind,
            provider_alias: input.client.providerAlias,
            oauth_credential_secret: input.client.oauthCredentialSecret,
          }
        : { mode: "fake" },
  };
}

export function toNativeProviderStateInput(
  state: BrainWakeProviderStateInput,
): NativeBrainWakeProviderStateInput {
  return {
    module_id: state.moduleId,
    strategy_id: state.strategyId,
    profile_fingerprint: state.profileFingerprint,
    provider_fingerprint: state.providerFingerprint,
    payload_version: state.payloadVersion,
    payload: state.payload,
    expires_at: state.expiresAt ?? undefined,
  };
}
