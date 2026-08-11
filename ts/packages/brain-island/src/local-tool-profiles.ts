import type {
  NativeBridgeModule,
  NativeSimpleKvRecord,
} from "@rusty-crew/native-bridge";
import {
  buildBuiltInToolCatalog,
  type BuiltInToolCatalog,
} from "./tool-registry.js";

const SCOPE_TYPE = "service";
const SCOPE_ID = "local_tool_profiles";

export interface LocalToolProfile {
  schemaVersion: 1;
  id: string;
  displayName: string;
  description?: string;
  enabled: boolean;
  system: boolean;
  readOnly: boolean;
  toolsets: string[];
  tools: string[];
  createdAt: string;
  updatedAt: string;
  revision?: number;
}

export interface LocalToolProfileWrite {
  id?: string;
  displayName?: string;
  description?: string;
  enabled?: boolean;
  toolsets?: string[];
  tools?: string[];
  expectedRevision?: number;
}

export interface LocalToolProfileList {
  schemaVersion: 1;
  catalogId: "local-tool-profiles";
  builtInCatalogId: BuiltInToolCatalog["catalogId"];
  items: LocalToolProfile[];
  total: number;
}

export interface LocalToolProfileStore {
  list(): Promise<LocalToolProfileList>;
  get(id: string): Promise<LocalToolProfile | undefined>;
  create(write: LocalToolProfileWrite): Promise<LocalToolProfile>;
  update(id: string, write: LocalToolProfileWrite): Promise<LocalToolProfile>;
  delete(id: string): Promise<LocalToolProfile>;
  resolve(id: string): Promise<{
    id: string;
    toolPolicy: {
      requestedToolsets: string[];
      requestedTools: string[];
    };
  }>;
}

export function createLocalToolProfileStore(input: {
  bridge: Pick<
    NativeBridgeModule,
    | "listSimpleKv"
    | "putSimpleKv"
    | "deleteSimpleKv"
    | "validateLocalToolProfilePolicy"
  >;
  now: () => string;
  catalog?: BuiltInToolCatalog;
}): LocalToolProfileStore {
  const catalog = input.catalog ?? buildBuiltInToolCatalog();
  return {
    async list() {
      await seedDefaultLocalToolProfiles(input.bridge, input.now, catalog);
      const items = await listProfiles(input.bridge);
      return {
        schemaVersion: 1,
        catalogId: "local-tool-profiles",
        builtInCatalogId: catalog.catalogId,
        items,
        total: items.length,
      };
    },
    async get(id) {
      await seedDefaultLocalToolProfiles(input.bridge, input.now, catalog);
      return getProfile(input.bridge, id);
    },
    async create(write) {
      const now = input.now();
      const id = stringValue(write.id) ?? "";
      const profile = normalizeProfileWrite(write, now, {
        id,
        system: false,
        readOnly: false,
        createdAt: now,
        revision: undefined,
      });
      await validateProfileReferences(input.bridge, profile, catalog);
      const existing = await getProfile(input.bridge, id);
      if (existing !== undefined) {
        throw new LocalToolProfileError(
          "local_tool_profile_exists",
          `local tool profile ${id} already exists`,
          409,
        );
      }
      return putProfile(input.bridge, profile, now);
    },
    async update(id, write) {
      const profileId = stringValue(id) ?? "";
      await validateProfileReferences(
        input.bridge,
        {
          id: profileId,
          enabled: true,
          system: false,
          readOnly: false,
          toolsets: [],
          tools: [],
        },
        catalog,
      );
      const current = await getProfile(input.bridge, profileId);
      if (current === undefined) {
        throw new LocalToolProfileError(
          "local_tool_profile_not_found",
          `local tool profile ${profileId} was not found`,
          404,
        );
      }
      if (current.readOnly) {
        throw new LocalToolProfileError(
          "local_tool_profile_read_only",
          `local tool profile ${profileId} is read-only`,
          409,
        );
      }
      assertExpectedRevision(current, write.expectedRevision);
      const now = input.now();
      const profile = normalizeProfileWrite(write, now, {
        id: profileId,
        system: current.system,
        readOnly: current.readOnly,
        createdAt: current.createdAt,
        revision: current.revision,
        current,
      });
      await validateProfileReferences(input.bridge, profile, catalog);
      return putProfile(input.bridge, profile, now);
    },
    async delete(id) {
      const profileId = requiredId(id);
      const current = await getProfile(input.bridge, profileId);
      if (current === undefined) {
        throw new LocalToolProfileError(
          "local_tool_profile_not_found",
          `local tool profile ${profileId} was not found`,
          404,
        );
      }
      if (current.readOnly) {
        throw new LocalToolProfileError(
          "local_tool_profile_read_only",
          `local tool profile ${profileId} is read-only`,
          409,
        );
      }
      if (current.revision === undefined) {
        throw new LocalToolProfileError(
          "local_tool_profile_revision_missing",
          `local tool profile ${profileId} has no DB revision`,
          409,
        );
      }
      const deleted = await input.bridge.deleteSimpleKv({
        scopeType: SCOPE_TYPE,
        scopeId: SCOPE_ID,
        key: profileId,
        expectedRevision: current.revision,
      });
      return profileFromRecord(deleted);
    },
    async resolve(id) {
      const profileId = requiredId(id);
      const profile = await this.get(profileId);
      if (profile === undefined) {
        throw new LocalToolProfileError(
          "local_tool_profile_not_found",
          `local tool profile ${profileId} was not found`,
          404,
        );
      }
      if (!profile.enabled) {
        throw new LocalToolProfileError(
          "local_tool_profile_disabled",
          `local tool profile ${profileId} is disabled`,
          409,
        );
      }
      return {
        id: profile.id,
        toolPolicy: {
          requestedToolsets: [...profile.toolsets],
          requestedTools: [...profile.tools],
        },
      };
    },
  };
}

export class LocalToolProfileError extends Error {
  readonly reasonCode: string;
  readonly statusCode: number;

  constructor(reasonCode: string, message: string, statusCode = 400) {
    super(message);
    this.name = "LocalToolProfileError";
    this.reasonCode = reasonCode;
    this.statusCode = statusCode;
  }
}

async function seedDefaultLocalToolProfiles(
  bridge: Pick<
    NativeBridgeModule,
    "listSimpleKv" | "putSimpleKv" | "validateLocalToolProfilePolicy"
  >,
  now: () => string,
  catalog: BuiltInToolCatalog,
): Promise<void> {
  const existing = new Map(
    (await listProfiles(bridge)).map((item) => [item.id, item]),
  );
  const timestamp = now();
  for (const profile of defaultLocalToolProfiles(timestamp, catalog)) {
    const current = existing.get(profile.id);
    const seedProfile =
      current === undefined
        ? profile
        : current.system && current.readOnly
          ? {
              ...profile,
              createdAt: current.createdAt,
              updatedAt: timestamp,
              revision: current.revision,
            }
          : undefined;
    if (seedProfile === undefined) continue;
    if (current !== undefined && profilesMatch(current, seedProfile)) continue;
    await validateProfileReferences(bridge, seedProfile, catalog);
    await putProfile(bridge, seedProfile, timestamp);
  }
}

function profilesMatch(
  left: LocalToolProfile,
  right: LocalToolProfile,
): boolean {
  return (
    left.displayName === right.displayName &&
    left.description === right.description &&
    left.enabled === right.enabled &&
    left.system === right.system &&
    left.readOnly === right.readOnly &&
    arraysMatch(left.toolsets, right.toolsets) &&
    arraysMatch(left.tools, right.tools)
  );
}

function arraysMatch(left: string[], right: string[]): boolean {
  if (left.length !== right.length) return false;
  return left.every((value, index) => value === right[index]);
}

function defaultLocalToolProfiles(
  now: string,
  catalog: BuiltInToolCatalog,
): LocalToolProfile[] {
  return [
    defaultProfile(now, {
      id: "basic_chat",
      displayName: "Basic Chat",
      description: "No built-in local tools.",
      toolsets: [],
      tools: [],
    }),
    defaultProfile(now, {
      id: "code_read",
      displayName: "Code Read",
      description: "Read-only local file and git inspection tools.",
      toolsets: ["local_code_read"],
      tools: [],
    }),
    defaultProfile(now, {
      id: "code_write",
      displayName: "Code Write",
      description: "Local code read/write tools including terminal and patch.",
      toolsets: ["local_code_read", "local_code_write"],
      tools: [],
    }),
    defaultProfile(now, {
      id: "worker_code_write",
      displayName: "Worker Code Write",
      description:
        "Delegated-worker write and patch tools that stay inside the worker workdir.",
      toolsets: ["local_code_read", "worker_code_write"],
      tools: [],
    }),
    defaultProfile(now, {
      id: "memory_skills",
      displayName: "Memory And Skills",
      description: "Profile memory, skills read, and session planning tools.",
      toolsets: ["memory_profile", "skills_read", "planning_session"],
      tools: [],
    }),
    defaultProfile(now, {
      id: "roleplay_lore",
      displayName: "Roleplay Lore",
      description: "Read, write, and manage roleplay lore layers.",
      toolsets: [
        "roleplay_lore_read",
        "roleplay_lore_write",
        "roleplay_lore_manage",
        "roleplay_scene_state",
      ],
      tools: [],
    }),
    defaultProfile(now, {
      id: "roleplay_mechanic",
      displayName: "Roleplay Mechanic",
      description:
        "Read-only roleplay diagnostics and proposal creation for mechanic sessions.",
      toolsets: ["roleplay_mechanic"],
      tools: [],
    }),
    defaultProfile(now, {
      id: "vision_playtester",
      displayName: "Vision Playtester",
      description:
        "Visible playtest session controls and completion reporting; this is task-focus friction, not a security sandbox.",
      toolsets: ["vision_playtester"],
      tools: ["deliver_completion_md"],
    }),
    defaultProfile(now, {
      id: "full_coding_agent",
      displayName: "Full Coding Agent",
      description:
        "Unrestricted code, research, memory, skills, planning, diagnostics, coordination, and delegation tools for normal coding work. Excludes roleplay, channels, curator controls, and worker-scoped tools.",
      toolsets: [
        "local_code_read",
        "local_code_write",
        "web_research",
        "browser",
        "browser_vision",
        "memory_external_read",
        "memory_external_write",
        "memory_profile",
        "skills_read",
        "skills_manage",
        "planning_session",
        "runtime_search",
        "storage_read",
        "diagnostics_read",
        "agent_coordination",
        "delegation_basic",
      ],
      tools: [],
    }),
    defaultProfile(now, {
      id: "full_agent",
      displayName: "Full Agent",
      description:
        "All built-in local tools for full agents and integration testing except explicitly workdir-scoped worker tools. MCP tools remain configured separately.",
      toolsets: fullAgentToolsets(catalog),
      tools: [],
    }),
  ];
}

function fullAgentToolsets(catalog: BuiltInToolCatalog): string[] {
  const fullAgentTools = new Set(
    catalog.tools
      .filter((tool) => !tool.safety.includes("workdir_scoped"))
      .map((tool) => tool.name),
  );
  return catalog.toolsets
    .filter((toolset) =>
      toolset.tools.some((toolName) => fullAgentTools.has(toolName)),
    )
    .map((toolset) => toolset.id);
}

function defaultProfile(
  now: string,
  input: Pick<
    LocalToolProfile,
    "id" | "displayName" | "description" | "toolsets" | "tools"
  >,
): LocalToolProfile {
  return {
    schemaVersion: 1,
    id: input.id,
    displayName: input.displayName,
    description: input.description,
    enabled: true,
    system: true,
    readOnly: true,
    toolsets: input.toolsets,
    tools: input.tools,
    createdAt: now,
    updatedAt: now,
  };
}

async function listProfiles(
  bridge: Pick<NativeBridgeModule, "listSimpleKv">,
): Promise<LocalToolProfile[]> {
  const records = await bridge.listSimpleKv({
    scopeType: SCOPE_TYPE,
    scopeId: SCOPE_ID,
    limit: 1_000,
  });
  return records
    .map(profileFromRecord)
    .sort((left, right) => left.id.localeCompare(right.id));
}

async function getProfile(
  bridge: Pick<NativeBridgeModule, "listSimpleKv">,
  id: string,
): Promise<LocalToolProfile | undefined> {
  const profileId = requiredId(id);
  const records = await bridge.listSimpleKv({
    scopeType: SCOPE_TYPE,
    scopeId: SCOPE_ID,
    keyPrefix: profileId,
    limit: 10,
  });
  return records.map(profileFromRecord).find((item) => item.id === profileId);
}

async function putProfile(
  bridge: Pick<NativeBridgeModule, "putSimpleKv">,
  profile: LocalToolProfile,
  now: string,
): Promise<LocalToolProfile> {
  const record = await bridge.putSimpleKv({
    scopeType: SCOPE_TYPE,
    scopeId: SCOPE_ID,
    key: profile.id,
    valueJson: JSON.stringify({
      ...profile,
      revision: undefined,
    }),
    now,
  });
  return profileFromRecord(record);
}

function profileFromRecord(record: NativeSimpleKvRecord): LocalToolProfile {
  const raw = JSON.parse(record.valueJson) as Record<string, unknown>;
  const id = requiredId(stringValue(raw.id) ?? record.key);
  return {
    schemaVersion: 1,
    id,
    displayName: stringValue(raw.displayName) ?? id,
    description: stringValue(raw.description),
    enabled: raw.enabled !== false,
    system: raw.system === true,
    readOnly: raw.readOnly === true,
    toolsets: stringArray(raw.toolsets),
    tools: stringArray(raw.tools),
    createdAt: stringValue(raw.createdAt) ?? record.createdAt,
    updatedAt: record.updatedAt,
    revision: record.revision,
  };
}

function normalizeProfileWrite(
  write: LocalToolProfileWrite,
  now: string,
  defaults: {
    id: string;
    system: boolean;
    readOnly: boolean;
    createdAt: string;
    revision?: number;
    current?: LocalToolProfile;
  },
): LocalToolProfile {
  const profile: LocalToolProfile = {
    schemaVersion: 1,
    id: defaults.id,
    displayName:
      write.displayName ?? defaults.current?.displayName ?? defaults.id,
    description: write.description ?? defaults.current?.description,
    enabled: write.enabled ?? defaults.current?.enabled ?? true,
    system: defaults.system,
    readOnly: defaults.readOnly,
    toolsets: write.toolsets ?? defaults.current?.toolsets ?? [],
    tools: write.tools ?? defaults.current?.tools ?? [],
    createdAt: defaults.createdAt,
    updatedAt: now,
    revision: defaults.revision,
  };
  return profile;
}

async function validateProfileReferences(
  bridge: Pick<NativeBridgeModule, "validateLocalToolProfilePolicy">,
  profile: Pick<LocalToolProfile, "id" | "toolsets" | "tools"> &
    Partial<Pick<LocalToolProfile, "enabled" | "system" | "readOnly">>,
  catalog: BuiltInToolCatalog,
): Promise<void> {
  const result = await bridge.validateLocalToolProfilePolicy({
    profile: {
      id: profile.id,
      enabled: "enabled" in profile ? profile.enabled === true : true,
      system: "system" in profile ? profile.system === true : false,
      readOnly: "readOnly" in profile ? profile.readOnly === true : false,
      toolsets: [...profile.toolsets],
      tools: [...profile.tools],
    },
    catalog: {
      toolsets: catalog.toolsets.map((item) => item.id),
      tools: catalog.tools.map((item) => item.name),
    },
  });
  if (result.ok) {
    return;
  }
  const issue = result.issues[0];
  throw new LocalToolProfileError(
    issue?.reasonCode ?? "local_tool_profile_invalid",
    issue?.message ?? `local tool profile ${profile.id} is invalid`,
  );
}

function assertExpectedRevision(
  current: LocalToolProfile,
  expectedRevision: number | undefined,
): void {
  if (expectedRevision === undefined) return;
  if (current.revision !== expectedRevision) {
    throw new LocalToolProfileError(
      "local_tool_profile_revision_mismatch",
      `local tool profile ${current.id} revision mismatch: expected ${expectedRevision}, found ${current.revision}`,
      409,
    );
  }
}

function requiredId(value: unknown): string {
  const id = stringValue(value);
  if (id === undefined) {
    throw new LocalToolProfileError(
      "local_tool_profile_id_required",
      "local tool profile id is required",
    );
  }
  if (!/^[A-Za-z0-9][A-Za-z0-9_.:-]{0,79}$/.test(id)) {
    throw new LocalToolProfileError(
      "local_tool_profile_invalid_id",
      "local tool profile id must start with a letter or number and contain only letters, numbers, underscore, dot, colon, or hyphen",
    );
  }
  return id;
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function stringArray(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return [
    ...new Set(
      value
        .filter((item): item is string => typeof item === "string")
        .map((item) => item.trim())
        .filter(Boolean),
    ),
  ].sort();
}
