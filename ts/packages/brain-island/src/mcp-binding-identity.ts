const SESSION_MATERIALIZATION_DELIMITER = "--session--";

/** Return the session-free desired template identity for a binding id. */
export function desiredMcpBindingTemplateId(bindingId: string): string {
  const index = bindingId.indexOf(SESSION_MATERIALIZATION_DELIMITER);
  return index < 1 ? bindingId : bindingId.slice(0, index);
}

/** Materialize one desired template for one exact authoritative session. */
export function materializedMcpBindingId(
  desiredBindingId: string,
  sessionId: string,
): string {
  return `${desiredMcpBindingTemplateId(desiredBindingId)}${SESSION_MATERIALIZATION_DELIMITER}${sessionId}`;
}
