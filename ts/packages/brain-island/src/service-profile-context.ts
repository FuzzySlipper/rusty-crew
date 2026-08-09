import type { NativeBridgeModule } from "@rusty-crew/native-bridge";

import {
  loadProfileContext,
  type LoadedProfileContext,
  type LoadProfileContextInput,
} from "./profile-loading.js";

export type LoadServiceProfileContextInput = LoadProfileContextInput & {
  bridge: Pick<NativeBridgeModule, "getProfileRegistryRecord">;
};

/**
 * Load filesystem-backed profile configuration while taking prompt assets from
 * the DB-backed registry record when one exists. Presence of the record is the
 * authority boundary: undefined DB prompt fields deliberately clear any stale
 * filesystem prompt asset instead of falling back to it.
 */
export async function loadServiceProfileContext(
  input: LoadServiceProfileContextInput,
): Promise<LoadedProfileContext> {
  const { bridge, ...profileInput } = input;
  const record = await bridge.getProfileRegistryRecord(String(input.profileId));
  return loadProfileContext({
    ...profileInput,
    ...(record === undefined
      ? {}
      : {
          profilePromptAssets: {
            soulMarkdown: record.promptSoulMarkdown,
            memoryMarkdown: record.promptMemoryMarkdown,
          },
        }),
  });
}
