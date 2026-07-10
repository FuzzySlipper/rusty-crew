import type { BrainWakeInput } from "./index.js";
import type { BrainHostContext } from "./brain-host-context.js";
import { effectiveWakeTimeoutMs } from "./service-runtime-config.js";
import { effectiveTurnTimeoutMs } from "./wake-timeout.js";

export function brainWakeTimeoutMs(
  context: BrainHostContext,
  wake: BrainWakeInput,
): number | undefined {
  const configuredSession = context.runtimeConfig?.sessions.find(
    (session) => session.sessionId === wake.sessionId,
  );
  return effectiveTurnTimeoutMs(
    effectiveWakeTimeoutMs({
      session: configuredSession,
      profile: context.profile.profile,
      service: context.runtimeConfig?.wakeTimeout,
    }),
  );
}
