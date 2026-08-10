export interface RoleplayCompactionEvidence {
  sceneId: string;
  sceneBrief?: string;
  relevantLore: Array<{
    sourceId: string;
    title: string;
    body: string;
  }>;
}

export function roleplayCompactionDomainContext(
  evidence: RoleplayCompactionEvidence | undefined,
): unknown {
  if (evidence === undefined) {
    return emptyRoleplayCompactionDomainContext();
  }
  const sceneBrief = evidence.sceneBrief?.trim();
  const lore = evidence.relevantLore
    .filter((source) => source.body.trim().length > 0)
    .slice(0, 8);
  const directorsNotes = [
    ...(sceneBrief
      ? [
          {
            noteId: `scene:${evidence.sceneId}`,
            text: `Preserve voice and emotional continuity. Current scene: ${sceneBrief.slice(0, 2_000)}`,
            provenanceSourceRefs: [],
          },
        ]
      : []),
    ...lore.map((source) => ({
      noteId: `lore:${source.sourceId}`,
      text: `${source.title}: ${source.body}`.slice(0, 2_000),
      provenanceSourceRefs: [],
    })),
  ];
  return {
    schemaVersion: 1,
    deriveSourceRefs: true,
    ...(sceneBrief
      ? {
          sceneBoundary: {
            sceneId: evidence.sceneId,
            sourceRefs: [],
            reason: "director_boundary",
            summary: sceneBrief.slice(0, 2_000),
          },
        }
      : {}),
    retentionTiers: [],
    directorsNotes,
    extractionRequests: lore.map((source) => ({
      requestId: `lore:${source.sourceId}`,
      kind: "lore_fact",
      sourceRefs: [],
    })),
  };
}

export function roleplayCompactionEvidenceFromMetadata(
  value: unknown,
  fallbackSceneId: string,
): RoleplayCompactionEvidence | undefined {
  if (!isRecord(value) || !isRecord(value.narratorDiagnostic)) {
    return undefined;
  }
  const diagnostic = value.narratorDiagnostic;
  const sceneBrief =
    typeof diagnostic.sceneBrief === "string"
      ? diagnostic.sceneBrief.trim()
      : undefined;
  const sourceIds = Array.isArray(diagnostic.relevantLoreRecordIds)
    ? diagnostic.relevantLoreRecordIds.filter(
        (sourceId): sourceId is string => typeof sourceId === "string",
      )
    : [];
  if (!sceneBrief && sourceIds.length === 0) return undefined;
  return {
    sceneId: fallbackSceneId,
    sceneBrief,
    relevantLore: sourceIds.map((sourceId) => ({
      sourceId,
      title: sourceId,
      body: "",
    })),
  };
}

export function emptyRoleplayCompactionDomainContext(): unknown {
  return {
    schemaVersion: 1,
    deriveSourceRefs: false,
    retentionTiers: [],
    directorsNotes: [],
    extractionRequests: [],
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
