export interface AgentMessageFileLink {
  readonly label: string;
  readonly pathWithLine: string;
}

const AGENT_MESSAGE_FILE_LINK =
  /(?<!!)\[([^\]\n]+)\]\((?:<(\/[^>\n]+)>|(\/[^)\n]+))\)/g;

export function replaceAgentMessageFileLinks(
  text: string,
  replacement: (link: AgentMessageFileLink) => string,
): string {
  return text.replace(
    AGENT_MESSAGE_FILE_LINK,
    (
      _match,
      label: string,
      anglePath: string | undefined,
      barePath: string | undefined,
    ) =>
      replacement({
        label,
        pathWithLine: (anglePath ?? barePath ?? "").trim(),
      }),
  );
}
