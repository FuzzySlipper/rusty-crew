export type ProviderRequestTimeoutModuleId =
  | "chat-completions"
  | "openai-responses";

const ENV_BY_MODULE: Record<ProviderRequestTimeoutModuleId, string> = {
  "chat-completions": "RUSTY_CREW_CHAT_COMPLETIONS_PROVIDER_REQUEST_TIMEOUT_MS",
  "openai-responses": "RUSTY_CREW_OPENAI_RESPONSES_PROVIDER_REQUEST_TIMEOUT_MS",
};

export function providerRequestTimeoutMs(
  moduleId: ProviderRequestTimeoutModuleId,
  env: Partial<NodeJS.ProcessEnv> = process.env,
): number | undefined {
  const variableName = ENV_BY_MODULE[moduleId];
  const normalized = env[variableName]?.trim().toLowerCase();
  if (
    normalized === undefined ||
    normalized === "" ||
    normalized === "0" ||
    normalized === "disabled" ||
    normalized === "none"
  ) {
    return undefined;
  }
  const configured = Number(normalized);
  if (Number.isSafeInteger(configured) && configured > 0) {
    return configured;
  }
  throw new Error(
    `${variableName} must be a positive integer, 0, disabled, or none`,
  );
}

export function providerRequestTimeoutDiagnostics(moduleId: string): {
  providerRequestTimeoutMode?: "disabled" | "configured";
  providerRequestTimeoutMs?: number;
} {
  if (moduleId !== "chat-completions" && moduleId !== "openai-responses") {
    return {};
  }
  const timeoutMs = providerRequestTimeoutMs(moduleId);
  return {
    providerRequestTimeoutMode:
      timeoutMs === undefined ? "disabled" : "configured",
    ...(timeoutMs === undefined ? {} : { providerRequestTimeoutMs: timeoutMs }),
  };
}
