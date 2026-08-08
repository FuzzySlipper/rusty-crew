import type { SessionWorkspace } from "@rusty-crew/contracts";

export interface RawSessionWorkspace {
  cwd: string;
  revision: number;
  updated_at: string;
}

export function toSessionWorkspace(
  workspace: RawSessionWorkspace,
): SessionWorkspace {
  return {
    cwd: workspace.cwd,
    revision: workspace.revision,
    updatedAt: workspace.updated_at,
  };
}
