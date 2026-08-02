import { existsSync } from "node:fs";
import { join } from "node:path";
import type {
  McpSurfaceDiagnostics,
  MemorySpaceDescriptor,
} from "@rusty-crew/contracts";
import { rustyCrewBuiltInSkill } from "./built-in-skills.js";

export type MemorySurfaceOwner = "crew" | "den" | "filesystem";
export type MemorySurfaceAvailability =
  | "available"
  | "degraded"
  | "unavailable"
  | "profile_scoped";

export interface MemorySurfaceCatalogItem {
  surfaceId: string;
  displayName: string;
  owner: MemorySurfaceOwner;
  storageHome: string;
  promptPolicy: string;
  modelFacingToolNames: string[];
  backendProvenance: string;
  availability: MemorySurfaceAvailability;
  availabilityReasonCode: string;
  lastSafeError?: string;
  notes: string[];
}

export interface MemorySurfaceCatalogProjection {
  generatedAt: string;
  items: MemorySurfaceCatalogItem[];
}

export interface MemorySurfaceCatalogInput {
  now: string;
  dataDir: string;
  profilesDir: string;
  skillsDir?: string;
  memorySpaceDescriptors: readonly MemorySpaceDescriptor[];
  storageSearchHealthy: boolean;
  externalMemory: {
    configured: boolean;
    clientAvailable: boolean;
    mode: "metadata";
    lastError?: string;
  };
  mcpSurfaces: readonly McpSurfaceDiagnostics[];
  denPlanningToolNames: readonly string[];
}

export function buildMemorySurfaceCatalog(
  input: MemorySurfaceCatalogInput,
): MemorySurfaceCatalogProjection {
  const descriptors = new Set(
    input.memorySpaceDescriptors.map((descriptor) => descriptor.space_id),
  );
  const skillsDir = input.skillsDir ?? join(input.profilesDir, "skills");
  const externalAvailability = input.externalMemory.configured
    ? input.externalMemory.clientAvailable
      ? availability("available", "memory_external_available")
      : availability(
          "unavailable",
          "memory_external_dependency_unavailable",
          input.externalMemory.lastError,
        )
    : availability(
        "unavailable",
        "memory_external_dependency_missing",
        input.externalMemory.lastError,
      );
  const denPlanning = denPlanningAvailability(
    input.mcpSurfaces,
    input.denPlanningToolNames,
  );

  return {
    generatedAt: input.now,
    items: [
      surface({
        surfaceId: "profile_prompt",
        displayName: "Profile prompt memory",
        owner: "crew",
        storageHome: "profile_registry.prompt_*",
        promptPolicy:
          "Profile instructions are assembled into the system prompt for that profile.",
        modelFacingToolNames: [],
        backendProvenance: "rusty-crew/profile-registry",
        ...availability("available", "profile_prompt_available"),
        notes: ["Edited through profile prompt administration APIs."],
      }),
      descriptorSurface(
        descriptors,
        "profile_dense",
        surface({
          surfaceId: "profile_dense",
          displayName: "Dense profile memory",
          owner: "crew",
          storageHome: "profile_memories",
          promptPolicy:
            "Bounded compact profile memory is injected when enabled for the profile.",
          modelFacingToolNames: [
            "dense_profile_memory",
            "memory_space_catalog",
            "memory_space_read",
          ],
          backendProvenance: "rusty-crew/core-memory-space",
          ...availability("available", "memory_space_descriptor_available"),
          notes: ["Compact stable profile/user memory."],
        }),
      ),
      descriptorSurface(
        descriptors,
        "session_memory",
        surface({
          surfaceId: "session_memory",
          displayName: "Session memory",
          owner: "crew",
          storageHome: "session_memory_records",
          promptPolicy:
            "Rust selects bounded active-branch and ancestor context when session memory is enabled.",
          modelFacingToolNames: ["memory_space_catalog", "memory_space_read"],
          backendProvenance: "rusty-crew/core-memory-space",
          ...availability("available", "memory_space_descriptor_available"),
          notes: ["Sibling branches are excluded unless explicitly requested."],
        }),
      ),
      surface({
        surfaceId: "memory_governance",
        displayName: "Memory proposals and governance",
        owner: "crew",
        storageHome: "memory_proposals, memory_governance_decisions",
        promptPolicy:
          "Governance records are not injected; bounded proposals are reviewed through typed admin APIs.",
        modelFacingToolNames: [],
        backendProvenance: "rusty-crew/core-memory-governance",
        ...availability("available", "memory_governance_available"),
        notes: [
          "Capture and curator decisions remain separate from external memory.",
        ],
      }),
      descriptorSurface(
        descriptors,
        "roleplay_lore",
        surface({
          surfaceId: "roleplay_lore",
          displayName: "Roleplay lore",
          owner: "crew",
          storageHome: "roleplay_lore_* module tables",
          promptPolicy:
            "Domain-specific lore is retrieved through typed lore and narrator operations.",
          modelFacingToolNames: [
            "recall_lore",
            "search_lore",
            "capture_lore_fact",
            "revise_lore_fact",
            "list_lore_layers",
            "get_lore_layer_config",
          ],
          backendProvenance: "rusty-crew/roleplay-lore-module",
          ...availability("available", "memory_space_descriptor_available"),
          notes: ["Lore keeps canon-aware domain governance and provenance."],
        }),
      ),
      surface({
        surfaceId: "runtime_search",
        displayName: "Runtime search",
        owner: "crew",
        storageHome: "runtime search index/read model",
        promptPolicy:
          "Never injected as generic memory; agents query prior session/runtime records explicitly.",
        modelFacingToolNames: ["session_search"],
        backendProvenance: "rusty-crew/core-runtime-search",
        ...(input.storageSearchHealthy
          ? availability("available", "runtime_search_available")
          : availability(
              "degraded",
              "runtime_search_unhealthy",
              "runtime storage diagnostics report search as unhealthy",
            )),
        notes: ["Transcript/runtime search is not durable profile memory."],
      }),
      surface({
        surfaceId: "external_memory",
        displayName: "External memory",
        owner: "den",
        storageHome: "configured external memory service",
        promptPolicy:
          "External memory context and tools are exposed only when configured and available.",
        modelFacingToolNames: [
          "memory_recall",
          "memory_search",
          "memory_read",
          "memory_store",
          "memory_propose",
        ],
        backendProvenance: "adapter-den/external-memory",
        ...externalAvailability,
        notes: [
          `Configured policy mode: ${input.externalMemory.mode}.`,
          "This surface does not contain Den documents, tasks, projects, or guidance.",
        ],
      }),
      surface({
        surfaceId: "den_planning",
        displayName: "Den documents, tasks, and guidance",
        owner: "den",
        storageHome: "profile-configured Den MCP servers",
        promptPolicy:
          "Never injected as memory; profile-scoped MCP tools provide explicit planning/document access.",
        modelFacingToolNames: [...new Set(input.denPlanningToolNames)].sort(),
        backendProvenance: "profile-mcp/den-planning",
        ...denPlanning,
        notes: [
          "Availability is profile-scoped rather than a service-wide implicit Den binding.",
        ],
      }),
      surface({
        surfaceId: "built_in_skills",
        displayName: "Built-in Rusty Crew help",
        owner: "crew",
        storageHome: "builtin://rusty-crew/skills",
        promptPolicy:
          "A small harness pointer is injected into native Crew role assembly; the full immutable skill body is loaded only through rusty_crew_help or skill_view.",
        modelFacingToolNames: ["rusty_crew_help", "skills_list", "skill_view"],
        backendProvenance: "rusty-crew/built-in-skill-catalog",
        ...availability("available", "built_in_skill_available"),
        notes: [
          `Reserved slug: ${rustyCrewBuiltInSkill.slug}.`,
          `Content version: ${rustyCrewBuiltInSkill.contentVersion}.`,
          `Content fingerprint: ${rustyCrewBuiltInSkill.contentFingerprint}.`,
          "Filesystem roots and profile skill filters cannot remove or shadow this skill.",
        ],
      }),
      surface({
        surfaceId: "skills",
        displayName: "Skills",
        owner: "filesystem",
        storageHome: skillsDir,
        promptPolicy:
          "Selected skill content is assembled for the profile; skill tools browse or manage configured roots.",
        modelFacingToolNames: ["skills_list", "skill_view", "skill_manage"],
        backendProvenance: "rusty-crew/filesystem-skills",
        ...(existsSync(skillsDir)
          ? availability("available", "skills_root_available")
          : availability(
              "degraded",
              "skills_root_missing",
              `configured skills root does not exist: ${skillsDir}`,
            )),
        notes: ["Skills are capability guidance, not durable memory records."],
      }),
      surface({
        surfaceId: "session_todo",
        displayName: "Session todo state",
        owner: "filesystem",
        storageHome: join(input.dataDir, "data", "session-todos"),
        promptPolicy:
          "Todo state is tool-driven local planning state and is not injected as durable memory.",
        modelFacingToolNames: ["todo"],
        backendProvenance: "rusty-crew/file-session-todo",
        ...availability("available", "session_todo_available"),
        notes: ["Todo entries are TTL-capable and session-scoped."],
      }),
    ],
  };
}

function descriptorSurface(
  descriptors: ReadonlySet<string>,
  descriptorId: string,
  item: MemorySurfaceCatalogItem,
): MemorySurfaceCatalogItem {
  if (descriptors.has(descriptorId)) return item;
  return {
    ...item,
    availability: "degraded",
    availabilityReasonCode: "memory_space_descriptor_missing",
    lastSafeError: `memory-space descriptor ${descriptorId} is not registered`,
  };
}

function denPlanningAvailability(
  surfaces: readonly McpSurfaceDiagnostics[],
  toolNames: readonly string[],
): Pick<
  MemorySurfaceCatalogItem,
  "availability" | "availabilityReasonCode" | "lastSafeError"
> {
  if (
    surfaces.some((surface) => surface.status === "active") &&
    toolNames.length > 0
  ) {
    return availability("profile_scoped", "den_planning_profile_tools_active");
  }
  const lastError = surfaces.find((surface) => surface.lastError)?.lastError;
  if (surfaces.some((surface) => surface.status === "degraded")) {
    return availability(
      "degraded",
      "den_planning_mcp_degraded",
      lastError ?? "one or more profile MCP surfaces are degraded",
    );
  }
  return availability(
    "profile_scoped",
    "den_planning_not_selected",
    "no active session currently exposes Den planning MCP tools",
  );
}

function availability(
  value: MemorySurfaceAvailability,
  reasonCode: string,
  lastSafeError?: string,
): Pick<
  MemorySurfaceCatalogItem,
  "availability" | "availabilityReasonCode" | "lastSafeError"
> {
  return {
    availability: value,
    availabilityReasonCode: reasonCode,
    ...(lastSafeError === undefined ? {} : { lastSafeError }),
  };
}

function surface(item: MemorySurfaceCatalogItem): MemorySurfaceCatalogItem {
  return item;
}
