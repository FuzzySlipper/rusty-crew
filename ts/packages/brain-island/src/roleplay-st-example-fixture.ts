import { readFileSync } from "node:fs";
import { join } from "node:path";

export const DEFAULT_ST_EXAMPLE_DIR = "/home/stash/st-example";

export interface StExampleFixture {
  exampleDir: string;
  manifest: Record<string, any>;
  characterCard: Record<string, any>;
  persona: Record<string, any>;
  lorebook: Record<string, any>;
  transcriptRows: Array<Record<string, any>>;
}

export interface RoleplayFirstResponseScenario {
  id: string;
  openingAssistant: string;
  firstUserReply: string;
  referenceAssistant: string;
  rubric: string[];
}

export interface RubricCheck {
  id: string;
  passed: boolean;
  description: string;
  evidence?: string;
}

export interface RoleplayEvaluationReport {
  scenarioId: string;
  totalChecks: number;
  passedChecks: number;
  score: number;
  checks: RubricCheck[];
  notableMisses: string[];
  promptStackTrace?: unknown;
  loreEvidence?: unknown;
  response: string;
}

export function loadStExampleFixture(
  exampleDir = DEFAULT_ST_EXAMPLE_DIR,
): StExampleFixture {
  return {
    exampleDir,
    manifest: readJson(exampleDir, "manifest.json"),
    characterCard: readJson(
      exampleDir,
      "Character Card - Crown Prince Xavier.json",
    ),
    persona: readJson(exampleDir, "Persona - Kopis Valliren.json"),
    lorebook: readJson(exampleDir, "Lorebook - LaDS_Philos.json"),
    transcriptRows: readFileSync(
      join(exampleDir, "Transcript - Crown Prince Xavier.jsonl"),
      "utf8",
    )
      .trim()
      .split(/\n/)
      .slice(1)
      .map((line) => JSON.parse(line)),
  };
}

export function buildStExampleImportPlan(options: {
  profileId: string;
  importId: string;
  sessionId: string;
  exampleDir?: string;
}): Record<string, unknown> {
  const fixture = loadStExampleFixture(options.exampleDir);
  const card = fixture.characterCard;
  const cardData = (card.data ?? card) as Record<string, any>;
  const persona = fixture.persona;
  return {
    profileId: options.profileId,
    importId: options.importId,
    provenance: {
      source: "st-example",
      package: fixture.manifest.package,
      generated: fixture.manifest.generated,
      manifestSha256: fixture.manifest.files,
    },
    rawSource: {
      manifest: fixture.manifest,
      presetFile: "Preset - Ava's Special.json",
      renderedPromptFile: "Rendered Prompt Export.txt",
    },
    character: {
      id: "st-crown-prince-xavier",
      name: cardData.name,
      description: cardData.description,
      personality: cardData.personality,
      scenario: cardData.scenario,
      firstMessage: cardData.first_mes,
      alternateGreetings: cardData.alternate_greetings ?? [],
      exampleMessages: [cardData.mes_example].filter(Boolean),
      tags: cardData.tags ?? [],
      rawMetadata: {
        spec: card.spec,
        spec_version: card.spec_version,
        creator: cardData.creator,
        extensions: cardData.extensions,
      },
    },
    persona: {
      id: "st-kopis-valliren",
      displayName: persona.name,
      description: persona.description,
      notes: persona.comment,
      rawMetadata: {
        spec: persona.spec,
        spec_version: persona.spec_version,
      },
    },
    loreLayer: {
      layerId: "st-lads-philos",
      name: "LaDS_Philos",
      description: "Imported SillyTavern lorebook from the ST example corpus.",
      purpose: "mixed",
      writePolicy: "manual",
    },
    loreEntries: Object.values(fixture.lorebook.entries).map((entry: any) => ({
      recordId: `st-lore-${entry.uid ?? entry.id}`,
      title: entry.comment || entry.name || `Lore ${entry.uid ?? entry.id}`,
      body: entry.content,
      worldId: options.profileId,
      entityId: entry.comment || entry.name,
      canonStatus: "draft",
      visibility: "public",
      primaryKeys: entry.key ?? entry.keys ?? [],
      secondaryKeys: entry.keysecondary ?? entry.secondary_keys ?? [],
      constant: entry.constant,
      enabled: entry.disable === true ? false : entry.enabled !== false,
      insertionOrder: entry.insertion_order ?? entry.order,
      probability:
        typeof entry.probability === "number" ? entry.probability / 100 : 1,
      rawMetadata: entry,
    })),
    session: {
      sessionId: options.sessionId,
      displayName: "Dark Xavier ST Example",
    },
    transcriptRows: fixture.transcriptRows.map((row: any, index: number) => ({
      role: row.is_system ? "system" : row.is_user ? "user" : "assistant",
      name: row.name,
      send_date: row.send_date,
      body: row.mes,
      swipe_id: row.swipe_id,
      swipes: Array.isArray(row.swipes) ? row.swipes : undefined,
      swipe_info: row.swipe_info,
      extra: row.extra,
      metadata: {
        // The example JSONL starts with one metadata-only header row. Preserve
        // the original file row instead of re-numbering the filtered messages.
        source_index: index + 1,
        is_user: row.is_user,
        is_system: row.is_system,
      },
    })),
  };
}

export function firstResponseScenario(
  fixture = loadStExampleFixture(),
): RoleplayFirstResponseScenario {
  const [openingAssistant, firstUserReply, referenceAssistant] =
    fixture.transcriptRows;
  if (!openingAssistant || !firstUserReply || !referenceAssistant) {
    throw new Error(
      "ST example transcript does not contain first-response rows",
    );
  }
  return {
    id: "dark-xavier-first-response",
    openingAssistant: String(openingAssistant.mes ?? ""),
    firstUserReply: String(firstUserReply.mes ?? ""),
    referenceAssistant: String(referenceAssistant.mes ?? ""),
    rubric: [
      "xavier_pov",
      "political_cover_story",
      "relationship_dynamic",
      "elevated_dark_fantasy_register",
      "continuity_with_user_reply",
      "clean_narrative_output",
    ],
  };
}

export function evaluateFirstResponse(
  response: string,
  options?: {
    promptStackTrace?: unknown;
    loreEvidence?: unknown;
    scenario?: RoleplayFirstResponseScenario;
  },
): RoleplayEvaluationReport {
  const scenario = options?.scenario ?? firstResponseScenario();
  const checks: RubricCheck[] = [
    check(
      "xavier_pov",
      /xavier|prince|crown prince/i.test(response),
      "Response keeps Crown Prince Xavier present as the assistant viewpoint.",
      evidence(response, /xavier|prince|crown prince/i),
    ),
    check(
      "political_cover_story",
      /veranthos|minister|lady|assignment|cover|court|steward/i.test(response),
      "Response preserves the political intrigue / cover-story pressure.",
      evidence(
        response,
        /veranthos|minister|lady|assignment|cover|court|steward/i,
      ),
    ),
    check(
      "relationship_dynamic",
      /kopis|bodyguard|guard|protect|closer|arm|shoulder/i.test(response),
      "Response recognizes the Kopis/Xavier bodyguard intimacy dynamic.",
      evidence(response, /kopis|bodyguard|guard|protect|closer|arm|shoulder/i),
    ),
    check(
      "elevated_dark_fantasy_register",
      /palace|chandelier|perfume|ambition|mask|blade|gilded|royal|crown/i.test(
        response,
      ) && response.length >= 500,
      "Response uses an elevated courtly/dark-fantasy register with enough density.",
      evidence(
        response,
        /palace|chandelier|perfume|ambition|mask|blade|gilded|royal|crown/i,
      ),
    ),
    check(
      "continuity_with_user_reply",
      /tea tray|assassin|veranthos|fond|purpose|tassel/i.test(response),
      "Response answers the first user reply instead of drifting to a generic opener.",
      evidence(response, /tea tray|assassin|veranthos|fond|purpose|tassel/i),
    ),
    check(
      "clean_narrative_output",
      !/```|tool_call|system:|assistant:|json/i.test(response),
      "Response avoids tool/debug/label artifacts.",
    ),
  ];
  const passedChecks = checks.filter((item) => item.passed).length;
  return {
    scenarioId: scenario.id,
    totalChecks: checks.length,
    passedChecks,
    score: passedChecks / checks.length,
    checks,
    notableMisses: checks
      .filter((item) => !item.passed)
      .map((item) => `${item.id}: ${item.description}`),
    promptStackTrace: options?.promptStackTrace,
    loreEvidence: options?.loreEvidence,
    response,
  };
}

function check(
  id: string,
  passed: boolean,
  description: string,
  evidenceValue?: string,
): RubricCheck {
  return {
    id,
    passed,
    description,
    ...(evidenceValue === undefined ? {} : { evidence: evidenceValue }),
  };
}

function evidence(response: string, pattern: RegExp): string | undefined {
  const match = pattern.exec(response);
  if (!match) return undefined;
  const start = Math.max(0, match.index - 80);
  const end = Math.min(response.length, match.index + match[0].length + 80);
  return response.slice(start, end).replace(/\s+/g, " ").trim();
}

function readJson(exampleDir: string, fileName: string): Record<string, any> {
  return JSON.parse(readFileSync(join(exampleDir, fileName), "utf8"));
}
