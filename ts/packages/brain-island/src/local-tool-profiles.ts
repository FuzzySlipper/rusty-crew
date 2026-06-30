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
    "listSimpleKv" | "putSimpleKv" | "deleteSimpleKv"
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
      const id = requiredId(write.id);
      const existing = await getProfile(input.bridge, id);
      if (existing !== undefined) {
        throw new LocalToolProfileError(
          "local_tool_profile_exists",
          `local tool profile ${id} already exists`,
          409,
        );
      }
      const profile = normalizeProfileWrite(write, catalog, now, {
        id,
        system: false,
        readOnly: false,
        createdAt: now,
        revision: undefined,
      });
      return putProfile(input.bridge, profile, now);
    },
    async update(id, write) {
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
      assertExpectedRevision(current, write.expectedRevision);
      const now = input.now();
      const profile = normalizeProfileWrite(write, catalog, now, {
        id: profileId,
        system: current.system,
        readOnly: current.readOnly,
        createdAt: current.createdAt,
        revision: current.revision,
        current,
      });
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
  bridge: Pick<NativeBridgeModule, "listSimpleKv" | "putSimpleKv">,
  now: () => string,
  catalog: BuiltInToolCatalog,
): Promise<void> {
  const existing = new Set((await listProfiles(bridge)).map((item) => item.id));
  const timestamp = now();
  for (const profile of defaultLocalToolProfiles(timestamp)) {
    if (existing.has(profile.id)) continue;
    validateProfileReferences(profile, catalog);
    await putProfile(bridge, profile, timestamp);
  }
}

function defaultLocalToolProfiles(now: string): LocalToolProfile[] {
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
      ],
      tools: [],
    }),
    defaultProfile(now, {
      id: "full_agent",
      displayName: "Full Agent",
      description:
        "Broad built-in local tools for full agents and integration testing. MCP tools remain configured separately.",
      toolsets: [
        "local_code_read",
        "local_code_write",
        "web_research",
        "browser",
        "browser_vision",
        "memory_den_read",
        "memory_den_write",
        "memory_profile",
        "skills_read",
        "skills_manage",
        "planning_session",
        "runtime_search",
        "storage_read",
        "diagnostics_read",
        "agent_coordination",
        "delegation_basic",
        "roleplay_lore_read",
        "roleplay_lore_write",
        "roleplay_lore_manage",
      ],
      tools: [],
    }),
  ];
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
  catalog: BuiltInToolCatalog,
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
  validateProfileReferences(profile, catalog);
  return profile;
}

function validateProfileReferences(
  profile: Pick<LocalToolProfile, "id" | "toolsets" | "tools">,
  catalog: BuiltInToolCatalog,
): void {
  requiredId(profile.id);
  const validToolsets = new Set(catalog.toolsets.map((item) => item.id));
  const validTools = new Set(catalog.tools.map((item) => item.name));
  for (const toolset of profile.toolsets) {
    if (toolset.startsWith("mcp:")) {
      throw new LocalToolProfileError(
        "local_tool_profile_rejects_mcp_toolset",
        `local tool profile ${profile.id} cannot reference dynamic MCP toolset ${toolset}`,
      );
    }
    if (!validToolsets.has(toolset)) {
      throw new LocalToolProfileError(
        "local_tool_profile_unknown_toolset",
        `local tool profile ${profile.id} references unknown built-in toolset ${toolset}`,
      );
    }
  }
  for (const tool of profile.tools) {
    if (!validTools.has(tool)) {
      throw new LocalToolProfileError(
        "local_tool_profile_unknown_tool",
        `local tool profile ${profile.id} references unknown built-in tool ${tool}`,
      );
    }
  }
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
