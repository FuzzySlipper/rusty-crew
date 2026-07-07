import { randomBytes } from "node:crypto";
import type { IncomingMessage } from "node:http";
import type {
  NativeRoleplayChatLayersWrite,
  NativeRoleplayLoreEntryPromotion,
  NativeRoleplayLoreFactCapture,
  NativeRoleplayLoreLayerWrite,
  NativeRoleplayLoreQuery,
  NativeRoleplayLoreReplace,
  NativeRoleplayLoreWrite,
} from "@rusty-crew/native-bridge";
import type { AdminRouteResult } from "../admin-diagnostics-api.js";
import { failure, successRoute } from "../service-route-results.js";
import type { RoleplayRouteContext } from "../service-roleplay-routes.js";

interface RoleplayChatLayersBrowserWrite extends NativeRoleplayChatLayersWrite {
  chat_id: string;
  layers: Array<{ layer_id: string; priority: number; enabled: boolean }>;
}

export interface RoleplayLoreRouteOptions {
  upsertSessionMetadata(
    sessionId: string,
    patch: { activeLayerIds?: string[] },
  ): Promise<unknown>;
}

export async function handleBrowserProfileLoreLayersRequest(
  request: IncomingMessage,
  state: RoleplayRouteContext,
  url: URL,
  profileId: string,
): Promise<AdminRouteResult> {
  if ((request.method ?? "GET").toUpperCase() === "GET") {
    return roleplayLoreLayerListResult(requestId(request), state, profileId);
  }
  if ((request.method ?? "GET").toUpperCase() === "POST") {
    const body = recordBody(await readJsonBody(request));
    const layer = await state.bridge.createLoreLayer(
      roleplayLoreLayerWriteFromBody(body, profileId, state.now()),
    );
    return successRoute(requestId(request), { layer });
  }
  return roleplayLoreMethodNotAllowed(
    requestId(request),
    "profile lore layer routes support GET and POST",
  );
}

export async function handleAdminRoleplayLoreRequest(
  request: IncomingMessage,
  state: RoleplayRouteContext,
  url: URL,
  options: RoleplayLoreRouteOptions,
): Promise<AdminRouteResult> {
  const requestIdValue = requestId(request);
  const method = (request.method ?? "GET").toUpperCase();
  try {
    if (url.pathname === "/v1/admin/roleplay/lore/layers") {
      if (method === "GET") {
        const profileId = url.searchParams.get("profile_id");
        if (!profileId) {
          return failure(400, requestIdValue, {
            code: "invalid_input",
            reason_code: "roleplay_lore_profile_id_required",
            message: "profile_id query parameter is required",
            retryable: false,
          });
        }
        return roleplayLoreLayerListResult(requestIdValue, state, profileId);
      }
      if (method === "POST") {
        const body = recordBody(await readJsonBody(request));
        return successRoute(requestIdValue, {
          layer: await state.bridge.createLoreLayer(
            roleplayLoreLayerWriteFromBody(body, undefined, state.now()),
          ),
        });
      }
      return roleplayLoreMethodNotAllowed(
        requestIdValue,
        "roleplay lore layer collection supports GET and POST",
      );
    }

    if (url.pathname === "/v1/admin/roleplay/lore/entries/search") {
      if (method !== "GET") {
        return roleplayLoreMethodNotAllowed(
          requestIdValue,
          "roleplay lore entry search supports GET",
        );
      }
      return successRoute(
        requestIdValue,
        await roleplayLoreEntrySearchResult(state, url),
      );
    }

    if (url.pathname === "/v1/admin/roleplay/lore/entries") {
      if (method !== "POST") {
        return roleplayLoreMethodNotAllowed(
          requestIdValue,
          "roleplay lore entry collection supports POST",
        );
      }
      return roleplayLoreEntryCreateResult(
        requestIdValue,
        state,
        recordBody(await readJsonBody(request)),
      );
    }

    const entryMatch = url.pathname.match(
      /^\/v1\/admin\/roleplay\/lore\/entries\/([^/]+)\/?$/,
    );
    if (entryMatch) {
      const entryId = decodeURIComponent(entryMatch[1]);
      if (method === "GET") {
        return roleplayLoreEntryDetailResult(
          requestIdValue,
          state,
          entryId,
          url,
        );
      }
      if (method === "PATCH") {
        return roleplayLoreEntryPatchResult(
          requestIdValue,
          state,
          entryId,
          recordBody(await readJsonBody(request)),
          url,
        );
      }
      return roleplayLoreMethodNotAllowed(
        requestIdValue,
        "roleplay lore entry item supports GET and PATCH",
      );
    }

    const entryPromoteMatch = url.pathname.match(
      /^\/v1\/admin\/roleplay\/lore\/entries\/([^/]+)\/promote\/?$/,
    );
    if (entryPromoteMatch) {
      const entryId = decodeURIComponent(entryPromoteMatch[1]);
      if (method !== "POST") {
        return roleplayLoreMethodNotAllowed(
          requestIdValue,
          "roleplay lore entry promote supports POST",
        );
      }
      return roleplayLoreEntryPromoteResult(
        requestIdValue,
        state,
        entryId,
        recordBody(await readJsonBody(request)),
        url,
      );
    }

    const layerEntriesMatch = url.pathname.match(
      /^\/v1\/admin\/roleplay\/lore\/layers\/([^/]+)\/entries\/?$/,
    );
    if (layerEntriesMatch) {
      if (method !== "GET") {
        return roleplayLoreMethodNotAllowed(
          requestIdValue,
          "roleplay lore layer entries supports GET",
        );
      }
      const layerId = decodeURIComponent(layerEntriesMatch[1]);
      const entries = await state.bridge.listEntriesByLayer(layerId);
      return successRoute(requestIdValue, {
        layerId,
        entries: entries.map(browserSafeLoreLayerEntry),
        total: entries.length,
      });
    }

    const layerEntryMatch = url.pathname.match(
      /^\/v1\/admin\/roleplay\/lore\/layers\/([^/]+)\/entries\/([^/]+)\/?$/,
    );
    if (layerEntryMatch) {
      const layerId = decodeURIComponent(layerEntryMatch[1]);
      const entryId = decodeURIComponent(layerEntryMatch[2]);
      if (method === "GET") {
        return roleplayLoreLayerEntryReadResult(
          requestIdValue,
          state,
          layerId,
          entryId,
        );
      }
      if (method === "PATCH") {
        return roleplayLoreLayerEntryPatchResult(
          requestIdValue,
          state,
          layerId,
          entryId,
          recordBody(await readJsonBody(request)),
        );
      }
      return roleplayLoreMethodNotAllowed(
        requestIdValue,
        "roleplay lore layer entry item supports GET and PATCH",
      );
    }

    const layerMatch = url.pathname.match(
      /^\/v1\/admin\/roleplay\/lore\/layers\/([^/]+)\/?$/,
    );
    if (layerMatch) {
      const layerId = decodeURIComponent(layerMatch[1]);
      if (method === "GET") {
        const layer = await state.bridge.getLoreLayer(layerId);
        if (layer === undefined) {
          return failure(404, requestIdValue, {
            code: "not_found",
            reason_code: "roleplay_lore_layer_not_found",
            message: `roleplay lore layer ${layerId} was not found`,
            retryable: false,
          });
        }
        const entries = await state.bridge.listEntriesByLayer(layerId);
        return successRoute(requestIdValue, {
          layer: withEntryCount(layer, entries.length),
          entryCount: entries.length,
        });
      }
      if (method === "PATCH") {
        const body = recordBody(await readJsonBody(request));
        return successRoute(requestIdValue, {
          layer: await state.bridge.updateLoreLayer(
            roleplayLoreLayerUpdateFromBody(body, layerId, state.now()),
          ),
        });
      }
      if (method === "DELETE") {
        return successRoute(requestIdValue, {
          layer: await state.bridge.archiveLoreLayer({
            layer_id: layerId,
            now: state.now(),
          }),
        });
      }
      return roleplayLoreMethodNotAllowed(
        requestIdValue,
        "roleplay lore layer item supports GET, PATCH, and DELETE",
      );
    }

    if (url.pathname === "/v1/admin/roleplay/lore/chat-layers/toggle") {
      if (method !== "POST") {
        return roleplayLoreMethodNotAllowed(
          requestIdValue,
          "roleplay lore chat layer toggle supports POST",
        );
      }
      const body = recordBody(await readJsonBody(request));
      const chatId = requiredRouteString(
        optionalString(body.chatId) ?? optionalString(body.chat_id),
        "chat_id",
      );
      const layerId = requiredRouteString(
        optionalString(body.layerId) ?? optionalString(body.layer_id),
        "layer_id",
      );
      const enabled =
        typeof body.enabled === "boolean"
          ? body.enabled
          : requiredRouteBoolean(undefined, "enabled");
      await state.bridge.toggleChatLayer({ chatId, layerId, enabled });
      return successRoute(requestIdValue, { chatId, layerId, enabled });
    }

    if (url.pathname === "/v1/admin/roleplay/lore/chat-layers/reorder") {
      if (method !== "POST") {
        return roleplayLoreMethodNotAllowed(
          requestIdValue,
          "roleplay lore chat layer reorder supports POST",
        );
      }
      const body = recordBody(await readJsonBody(request));
      const chatId = requiredRouteString(
        optionalString(body.chatId) ?? optionalString(body.chat_id),
        "chat_id",
      );
      const layerIds = stringArray(
        body.layerIds ?? body.layer_ids,
        "layer_ids",
      );
      await state.bridge.reorderChatLayers({ chatId, layerIds });
      return successRoute(requestIdValue, { chatId, layerIds });
    }

    if (url.pathname === "/v1/admin/roleplay/lore/chat-layers") {
      if (method === "GET") {
        const chatId = url.searchParams.get("chat_id");
        if (!chatId) {
          return failure(400, requestIdValue, {
            code: "invalid_input",
            reason_code: "roleplay_lore_chat_id_required",
            message: "chat_id query parameter is required",
            retryable: false,
          });
        }
        const layers = await state.bridge.getChatLayers(chatId);
        return successRoute(requestIdValue, {
          chatId,
          layers,
          activeLayerIds: layers
            .filter((layer) => layer.enabled !== false)
            .sort(
              (left, right) => Number(left.priority) - Number(right.priority),
            )
            .map((layer) => String(layer.layer_id)),
        });
      }
      if (method !== "POST") {
        return roleplayLoreMethodNotAllowed(
          requestIdValue,
          "roleplay lore chat layers supports GET and POST",
        );
      }
      const body = recordBody(await readJsonBody(request));
      const write = roleplayChatLayersWriteFromBody(body, state.now());
      await state.bridge.setChatLayers(write);
      await options.upsertSessionMetadata(write.chat_id, {
        activeLayerIds: write.layers.map((layer) => layer.layer_id),
      });
      return successRoute(requestIdValue, {
        saved: true,
        chatId: write.chat_id,
      });
    }

    if (url.pathname === "/v1/admin/roleplay/lore/facts/capture") {
      if (method !== "POST") {
        return roleplayLoreMethodNotAllowed(
          requestIdValue,
          "roleplay lore fact capture supports POST",
        );
      }
      const body = (await readJsonBody(
        request,
      )) as NativeRoleplayLoreFactCapture;
      return successRoute(requestIdValue, {
        entry: await state.bridge.captureLoreFact(body),
      });
    }

    return failure(404, requestIdValue, {
      code: "not_found",
      reason_code: "unknown_roleplay_lore_admin_route",
      message: `unknown roleplay lore admin route ${url.pathname}`,
      retryable: false,
    });
  } catch (error) {
    return failure(400, requestIdValue, {
      code: "invalid_input",
      reason_code: "roleplay_lore_admin_failed",
      message: errorMessage(error, "roleplay lore admin request failed"),
      retryable: false,
    });
  }
}

function roleplayLoreMethodNotAllowed(
  requestIdValue: string,
  message: string,
): AdminRouteResult {
  return failure(405, requestIdValue, {
    code: "method_not_allowed",
    reason_code: "roleplay_lore_method_not_allowed",
    message,
    retryable: false,
  });
}

async function roleplayLoreLayerListResult(
  requestIdValue: string,
  state: RoleplayRouteContext,
  profileId: string,
): Promise<AdminRouteResult> {
  const layers = await state.bridge.listLoreLayers(profileId);
  const counts = await Promise.all(
    layers.map(async (layer) => {
      const layerId = String(layer.layer_id);
      return [
        layerId,
        await state.bridge
          .listEntriesByLayer(layerId)
          .then((entries) => entries.length)
          .catch(() => 0),
      ] as const;
    }),
  );
  const entryCounts = Object.fromEntries(counts);
  return successRoute(requestIdValue, {
    profileId,
    layers: layers.map((layer) =>
      withEntryCount(layer, entryCounts[String(layer.layer_id)] ?? 0),
    ),
    entryCounts,
    total: layers.length,
  });
}

async function roleplayLoreEntrySearchResult(
  state: RoleplayRouteContext,
  url: URL,
): Promise<Record<string, unknown>> {
  const params = url.searchParams;
  const profileId = optionalString(params.get("profile_id"));
  const chatId = optionalString(params.get("chat_id"));
  const explicitLayerIds = roleplayLoreSearchLayerIds(params);
  const layerScope = await roleplayLoreSearchLayerScope(state, {
    profileId,
    chatId,
    explicitLayerIds,
  });
  const page = roleplayLoreSearchPage(params);
  const query = roleplayLoreSearchQuery(params, page);
  const pageResult =
    layerScope.recordIds === undefined
      ? await queryUnscopedRoleplayLoreEntrySearchPage(state, query, page)
      : await queryLayerScopedRoleplayLoreEntrySearchPage(
          state,
          query,
          page,
          layerScope.recordIds,
        );
  return {
    query: {
      text:
        optionalString(params.get("q")) ?? optionalString(params.get("query")),
      profileId,
      chatId,
      layerIds: layerScope.layerIds,
      worldId: optionalString(params.get("world_id") ?? params.get("worldId")),
      entityId: optionalString(
        params.get("entity_id") ?? params.get("entityId"),
      ),
      canonStatus: optionalString(
        params.get("canon_status") ?? params.get("canonStatus"),
      ),
      visibility: optionalString(params.get("visibility")),
      shapeId: optionalString(params.get("shape_id") ?? params.get("shapeId")),
      includeSuperseded: optionalBooleanQuery(params, "include_superseded"),
      includeTombstoned: optionalBooleanQuery(params, "include_tombstoned"),
    },
    entries: pageResult.entries.map(browserSafeLoreEntry),
    layers: layerScope.layers.map((layer) => withEntryCount(layer, 0)),
    layerContext: {
      source: layerScope.source,
      profileId,
      chatId,
      layerIds: layerScope.layerIds,
      activeLayerIds: layerScope.activeLayerIds,
    },
    total: pageResult.total,
    totalExact: pageResult.totalExact,
    limit: page.limit,
    offset: page.offset,
    hasMore: pageResult.hasMore,
  };
}

async function roleplayLoreEntryCreateResult(
  requestIdValue: string,
  state: RoleplayRouteContext,
  body: Record<string, unknown>,
): Promise<AdminRouteResult> {
  const layerId = requiredRouteString(
    optionalString(body.layer_id) ?? optionalString(body.layerId),
    "layer_id",
  );
  const layer = await state.bridge.getLoreLayer(layerId);
  if (layer === undefined) {
    return failure(404, requestIdValue, {
      code: "not_found",
      reason_code: "roleplay_lore_layer_not_found",
      message: `roleplay lore layer ${layerId} was not found`,
      retryable: false,
    });
  }
  if (layer.is_archived === true) {
    return failure(409, requestIdValue, {
      code: "conflict",
      reason_code: "roleplay_lore_layer_archived",
      message: `roleplay lore layer ${layerId} is archived`,
      retryable: false,
    });
  }
  if (layer.write_policy === "readonly") {
    return failure(409, requestIdValue, {
      code: "conflict",
      reason_code: "roleplay_lore_layer_readonly",
      message: `roleplay lore layer ${layerId} is readonly`,
      retryable: false,
    });
  }

  const write = roleplayLoreWriteFromBody(
    roleplayLoreWriteBodyFromRequest(body),
    undefined,
    state.now(),
    {
      defaultRecordId: `lore-${randomBytes(8).toString("hex")}`,
      defaultSource: "ui",
      defaultDurabilityRationale:
        "Manual roleplay lore entry created through browser admin API.",
    },
  );
  const entry = await state.bridge.addLoreEntry(write);
  const entryControls = roleplayLoreControlsForEntry(entry);
  await state.bridge.addEntryToLayer({
    layer_id: layerId,
    record_id: String(entry.record_id),
    is_constant:
      optionalBoolean(body.is_constant) ??
      optionalBoolean(body.isConstant) ??
      optionalBoolean(body.constant) ??
      optionalBoolean(entryControls.constant) ??
      false,
    priority: Math.trunc(
      optionalNumber(body.priority) ??
        optionalNumber(body.insertion_order) ??
        optionalNumber(body.insertionOrder) ??
        optionalNumber(entryControls.insertion_order) ??
        0,
    ),
    added_at: write.now,
  });
  return successRoute(requestIdValue, {
    ...(await roleplayLoreEntryDetailData(state, entry, {
      explicitLayerIds: [layerId],
    })),
    created: true,
  });
}

async function roleplayLoreEntryDetailResult(
  requestIdValue: string,
  state: RoleplayRouteContext,
  entryId: string,
  url: URL,
): Promise<AdminRouteResult> {
  const entry = await state.bridge.getLoreEntry(entryId);
  if (entry === undefined) {
    return failure(404, requestIdValue, {
      code: "not_found",
      reason_code: "roleplay_lore_entry_not_found",
      message: `roleplay lore entry ${entryId} was not found`,
      retryable: false,
    });
  }
  return successRoute(
    requestIdValue,
    await roleplayLoreEntryDetailData(state, entry, {
      profileId: optionalString(url.searchParams.get("profile_id")),
      chatId: optionalString(url.searchParams.get("chat_id")),
      explicitLayerIds: roleplayLoreSearchLayerIds(url.searchParams),
    }),
  );
}

async function roleplayLoreEntryPatchResult(
  requestIdValue: string,
  state: RoleplayRouteContext,
  entryId: string,
  body: Record<string, unknown>,
  url: URL,
): Promise<AdminRouteResult> {
  const existing = await state.bridge.getLoreEntry(entryId);
  if (existing === undefined) {
    return failure(404, requestIdValue, {
      code: "not_found",
      reason_code: "roleplay_lore_entry_not_found",
      message: `roleplay lore entry ${entryId} was not found`,
      retryable: false,
    });
  }
  const replace: NativeRoleplayLoreReplace = {
    write: roleplayLoreWriteFromBody(
      roleplayLoreWriteBodyFromRequest(body),
      existing,
      state.now(),
      {
        forcedRecordId: entryId,
        defaultSource: "ui",
        defaultDurabilityRationale:
          "Roleplay lore entry updated through browser admin API.",
      },
    ),
    expected_revision: requiredPositiveInteger(
      body.expected_revision ?? body.expectedRevision,
      "expected_revision",
    ),
  };
  const entry = await state.bridge.replaceLoreEntry(replace);
  return successRoute(
    requestIdValue,
    await roleplayLoreEntryDetailData(state, entry, {
      profileId: optionalString(url.searchParams.get("profile_id")),
      chatId: optionalString(url.searchParams.get("chat_id")),
      explicitLayerIds: roleplayLoreSearchLayerIds(url.searchParams),
    }),
  );
}

async function roleplayLoreEntryPromoteResult(
  requestIdValue: string,
  state: RoleplayRouteContext,
  entryId: string,
  body: Record<string, unknown>,
  url: URL,
): Promise<AdminRouteResult> {
  const sourceEntry = await state.bridge.getLoreEntry(entryId);
  if (sourceEntry === undefined) {
    return failure(404, requestIdValue, {
      code: "not_found",
      reason_code: "roleplay_lore_entry_not_found",
      message: `roleplay lore entry ${entryId} was not found`,
      retryable: false,
    });
  }

  const targetLayerId = requiredRouteString(
    optionalString(body.targetLayerId) ?? optionalString(body.target_layer_id),
    "target_layer_id",
  );
  const targetLayer = await state.bridge.getLoreLayer(targetLayerId);
  if (targetLayer === undefined) {
    return failure(404, requestIdValue, {
      code: "not_found",
      reason_code: "roleplay_lore_target_layer_not_found",
      message: `roleplay lore target layer ${targetLayerId} was not found`,
      retryable: false,
    });
  }
  if (targetLayer.is_archived === true) {
    return failure(409, requestIdValue, {
      code: "conflict",
      reason_code: "roleplay_lore_target_layer_archived",
      message: `roleplay lore target layer ${targetLayerId} is archived`,
      retryable: false,
    });
  }
  if (targetLayer.write_policy === "readonly") {
    return failure(409, requestIdValue, {
      code: "conflict",
      reason_code: "roleplay_lore_target_layer_readonly",
      message: `roleplay lore target layer ${targetLayerId} is readonly`,
      retryable: false,
    });
  }

  const sourceLayerId = await roleplayLorePromotionSourceLayerId(
    requestIdValue,
    state,
    entryId,
    body,
    url,
  );
  if (typeof sourceLayerId !== "string") return sourceLayerId;

  const newRecordId =
    optionalString(body.newRecordId) ??
    optionalString(body.new_record_id) ??
    `lore-promoted-${randomBytes(8).toString("hex")}`;
  if ((await state.bridge.getLoreEntry(newRecordId)) !== undefined) {
    return failure(409, requestIdValue, {
      code: "conflict",
      reason_code: "roleplay_lore_promoted_entry_exists",
      message: `roleplay lore promoted entry ${newRecordId} already exists`,
      retryable: false,
    });
  }

  const now = optionalString(body.now) ?? state.now();
  const promotion: NativeRoleplayLoreEntryPromotion = {
    source_layer_id: sourceLayerId,
    source_record_id: entryId,
    target_layer_id: targetLayerId,
    new_record_id: newRecordId,
    is_constant:
      optionalBoolean(body.is_constant) ??
      optionalBoolean(body.isConstant) ??
      false,
    priority: Math.trunc(optionalNumber(body.priority) ?? 0),
    now,
  };
  await state.bridge.promoteLoreEntry(promotion);
  const promotedEntry = await state.bridge.getLoreEntry(newRecordId);
  if (promotedEntry === undefined) {
    return failure(500, requestIdValue, {
      code: "internal_error",
      reason_code: "roleplay_lore_promoted_entry_unreadable",
      message: `roleplay lore promoted entry ${newRecordId} was not readable`,
      retryable: false,
    });
  }
  return successRoute(requestIdValue, {
    ...(await roleplayLoreEntryDetailData(state, promotedEntry, {
      profileId: optionalString(url.searchParams.get("profile_id")),
      chatId: optionalString(url.searchParams.get("chat_id")),
      explicitLayerIds: [targetLayerId],
    })),
    promoted: true,
    source: {
      layerId: sourceLayerId,
      recordId: entryId,
      entry: browserSafeLoreEntry(sourceEntry),
    },
    target: {
      layerId: targetLayerId,
      recordId: newRecordId,
    },
  });
}

async function roleplayLoreLayerEntryReadResult(
  requestIdValue: string,
  state: RoleplayRouteContext,
  layerId: string,
  entryId: string,
): Promise<AdminRouteResult> {
  const layerEntry = await roleplayLoreLayerEntryById(state, layerId, entryId);
  if (layerEntry === undefined) {
    return failure(404, requestIdValue, {
      code: "not_found",
      reason_code: "roleplay_lore_layer_entry_not_found",
      message: `roleplay lore entry ${entryId} was not found in layer ${layerId}`,
      retryable: false,
    });
  }
  return successRoute(requestIdValue, {
    layerId,
    recordId: entryId,
    layerEntry: browserSafeLoreLayerEntry(layerEntry),
  });
}

async function roleplayLoreLayerEntryPatchResult(
  requestIdValue: string,
  state: RoleplayRouteContext,
  layerId: string,
  entryId: string,
  body: Record<string, unknown>,
): Promise<AdminRouteResult> {
  const existing = await roleplayLoreLayerEntryById(state, layerId, entryId);
  if (existing === undefined) {
    return failure(404, requestIdValue, {
      code: "not_found",
      reason_code: "roleplay_lore_layer_entry_not_found",
      message: `roleplay lore entry ${entryId} was not found in layer ${layerId}`,
      retryable: false,
    });
  }
  await state.bridge.addEntryToLayer(
    roleplayLoreLayerEntryLinkFromBody(body, existing, state.now()),
  );
  const updated = await roleplayLoreLayerEntryById(state, layerId, entryId);
  if (updated === undefined) {
    return failure(500, requestIdValue, {
      code: "internal_error",
      reason_code: "roleplay_lore_layer_entry_unreadable",
      message: `roleplay lore layer entry ${layerId}:${entryId} was not readable after update`,
      retryable: false,
    });
  }
  return successRoute(requestIdValue, {
    layerId,
    recordId: entryId,
    layerEntry: browserSafeLoreLayerEntry(updated),
    updated: true,
  });
}

async function roleplayLoreLayerEntryById(
  state: RoleplayRouteContext,
  layerId: string,
  entryId: string,
): Promise<Record<string, unknown> | undefined> {
  const entries = await state.bridge.listEntriesByLayer(layerId);
  return entries.find((entry) => String(entry.record_id) === entryId);
}

async function roleplayLorePromotionSourceLayerId(
  requestIdValue: string,
  state: RoleplayRouteContext,
  entryId: string,
  body: Record<string, unknown>,
  url: URL,
): Promise<string | AdminRouteResult> {
  const explicitSourceLayerId =
    optionalString(body.sourceLayerId) ??
    optionalString(body.source_layer_id) ??
    optionalString(url.searchParams.get("source_layer_id")) ??
    optionalString(url.searchParams.get("sourceLayerId"));
  if (explicitSourceLayerId !== undefined) {
    return roleplayLorePromotionValidatedSourceLayerId(
      requestIdValue,
      state,
      entryId,
      explicitSourceLayerId,
    );
  }

  const candidateLayerIds = await roleplayLorePromotionCandidateLayerIds(
    state,
    body,
    url,
  );
  if (candidateLayerIds.length === 0) {
    return failure(400, requestIdValue, {
      code: "invalid_input",
      reason_code: "roleplay_lore_source_layer_required",
      message:
        "source_layer_id is required when profile_id, chat_id, or source layer scope is not provided",
      retryable: false,
    });
  }
  const containingLayerIds = (
    await Promise.all(
      candidateLayerIds.map(async (layerId) => {
        if ((await state.bridge.getLoreLayer(layerId)) === undefined) {
          return undefined;
        }
        const entries = await state.bridge.listEntriesByLayer(layerId);
        return entries.some((entry) => String(entry.record_id) === entryId)
          ? layerId
          : undefined;
      }),
    )
  ).filter((layerId): layerId is string => layerId !== undefined);
  if (containingLayerIds.length === 1) return containingLayerIds[0]!;
  if (containingLayerIds.length === 0) {
    return failure(404, requestIdValue, {
      code: "not_found",
      reason_code: "roleplay_lore_source_layer_not_found",
      message: `roleplay lore entry ${entryId} was not found in the provided source layer scope`,
      retryable: false,
    });
  }
  return failure(409, requestIdValue, {
    code: "conflict",
    reason_code: "roleplay_lore_source_layer_ambiguous",
    message: `roleplay lore entry ${entryId} exists in multiple source layers; source_layer_id is required`,
    retryable: false,
  });
}

async function roleplayLorePromotionValidatedSourceLayerId(
  requestIdValue: string,
  state: RoleplayRouteContext,
  entryId: string,
  sourceLayerId: string,
): Promise<string | AdminRouteResult> {
  if ((await state.bridge.getLoreLayer(sourceLayerId)) === undefined) {
    return failure(404, requestIdValue, {
      code: "not_found",
      reason_code: "roleplay_lore_source_layer_not_found",
      message: `roleplay lore source layer ${sourceLayerId} was not found`,
      retryable: false,
    });
  }
  const entries = await state.bridge.listEntriesByLayer(sourceLayerId);
  if (!entries.some((entry) => String(entry.record_id) === entryId)) {
    return failure(404, requestIdValue, {
      code: "not_found",
      reason_code: "roleplay_lore_source_entry_not_in_layer",
      message: `roleplay lore entry ${entryId} was not found in source layer ${sourceLayerId}`,
      retryable: false,
    });
  }
  return sourceLayerId;
}

async function roleplayLorePromotionCandidateLayerIds(
  state: RoleplayRouteContext,
  body: Record<string, unknown>,
  url: URL,
): Promise<string[]> {
  const explicitLayerIds = [
    ...stringListField(body, ["sourceLayerIds", "source_layer_ids"]),
    ...url.searchParams.getAll("source_layer_id"),
    ...url.searchParams.getAll("sourceLayerId"),
    ...url.searchParams.getAll("source_layer_ids"),
    ...url.searchParams.getAll("sourceLayerIds"),
  ]
    .flatMap((value) => value.split(","))
    .map((value) => value.trim())
    .filter((value) => value.length > 0);
  if (explicitLayerIds.length > 0) return [...new Set(explicitLayerIds)];

  const profileId =
    optionalString(body.profileId) ??
    optionalString(body.profile_id) ??
    optionalString(url.searchParams.get("profile_id")) ??
    optionalString(url.searchParams.get("profileId"));
  if (profileId !== undefined) {
    const layers = await state.bridge.listLoreLayers(profileId);
    return layers.map((layer) => String(layer.layer_id));
  }

  const chatId =
    optionalString(body.chatId) ??
    optionalString(body.chat_id) ??
    optionalString(url.searchParams.get("chat_id")) ??
    optionalString(url.searchParams.get("chatId"));
  if (chatId !== undefined) {
    const layers = await state.bridge.getChatLayers(chatId);
    return layers
      .filter((layer) => layer.enabled !== false)
      .sort((left, right) => Number(left.priority) - Number(right.priority))
      .map((layer) => String(layer.layer_id));
  }

  return [];
}

async function roleplayLoreEntryDetailData(
  state: RoleplayRouteContext,
  entry: Record<string, unknown>,
  scope: {
    profileId?: string;
    chatId?: string;
    explicitLayerIds: readonly string[];
  },
): Promise<Record<string, unknown>> {
  const entryId = String(entry.record_id);
  const [provenance, layerContext, supersedes, supersededBy] =
    await Promise.all([
      state.bridge.loreEntryProvenanceEvents(entryId),
      roleplayLoreEntryLayerContext(state, entryId, scope),
      optionalString(entry.supersedes_record_id) === undefined
        ? Promise.resolve(undefined)
        : state.bridge.getLoreEntry(String(entry.supersedes_record_id)),
      optionalString(entry.superseded_by_record_id) === undefined
        ? Promise.resolve(undefined)
        : state.bridge.getLoreEntry(String(entry.superseded_by_record_id)),
    ]);
  return {
    entry: browserSafeLoreEntry(entry),
    provenance,
    supersession: {
      supersedesRecordId: optionalString(entry.supersedes_record_id),
      supersededByRecordId: optionalString(entry.superseded_by_record_id),
      supersedes: supersedes ? browserSafeLoreEntry(supersedes) : undefined,
      supersededBy: supersededBy
        ? browserSafeLoreEntry(supersededBy)
        : undefined,
    },
    layerEntries: layerContext.layerEntries.map(browserSafeLoreLayerEntry),
    layers: layerContext.layers,
    layerContext: {
      source: layerContext.source,
      profileId: scope.profileId,
      chatId: scope.chatId,
      layerIds: layerContext.layerIds,
      activeLayerIds: layerContext.activeLayerIds,
    },
  };
}

async function roleplayLoreEntryLayerContext(
  state: RoleplayRouteContext,
  entryId: string,
  scope: {
    profileId?: string;
    chatId?: string;
    explicitLayerIds: readonly string[];
  },
): Promise<{
  source: "explicit" | "chat" | "profile" | "all";
  layerIds: string[];
  activeLayerIds: string[];
  layers: Record<string, unknown>[];
  layerEntries: Record<string, unknown>[];
}> {
  const layerScope = await roleplayLoreSearchLayerScope(state, scope);
  const layerEntries =
    layerScope.layerIds.length === 0
      ? []
      : (
          await Promise.all(
            layerScope.layerIds.map((layerId) =>
              state.bridge.listEntriesByLayer(layerId),
            ),
          )
        )
          .flat()
          .filter((entry) => String(entry.record_id) === entryId);
  return {
    source: layerScope.source,
    layerIds: layerScope.layerIds,
    activeLayerIds: layerScope.activeLayerIds,
    layers: layerScope.layers,
    layerEntries,
  };
}

function roleplayLoreSearchQuery(
  params: URLSearchParams,
  page: { limit: number; offset: number },
): NativeRoleplayLoreQuery {
  return {
    world_id: optionalString(params.get("world_id") ?? params.get("worldId")),
    entity_id: optionalString(
      params.get("entity_id") ?? params.get("entityId"),
    ),
    canon_status: optionalString(
      params.get("canon_status") ?? params.get("canonStatus"),
    ),
    visibility: optionalString(params.get("visibility")),
    shape_id: optionalString(params.get("shape_id") ?? params.get("shapeId")),
    query:
      optionalString(params.get("q")) ?? optionalString(params.get("query")),
    include_superseded:
      optionalBooleanQuery(params, "include_superseded") ??
      optionalBooleanQuery(params, "includeSuperseded") ??
      false,
    include_tombstoned:
      optionalBooleanQuery(params, "include_tombstoned") ??
      optionalBooleanQuery(params, "includeTombstoned") ??
      false,
    page,
  };
}

async function queryUnscopedRoleplayLoreEntrySearchPage(
  state: RoleplayRouteContext,
  query: NativeRoleplayLoreQuery,
  page: { limit: number; offset: number },
): Promise<{
  entries: Record<string, unknown>[];
  total: number;
  totalExact: boolean;
  hasMore: boolean;
}> {
  const rawEntries = await state.bridge.queryLoreEntries({
    ...query,
    page: {
      limit: page.limit + 1,
      offset: page.offset,
    },
  });
  const hasMore = rawEntries.length > page.limit;
  const entries = rawEntries.slice(0, page.limit);
  return {
    entries,
    total: page.offset + entries.length + (hasMore ? 1 : 0),
    totalExact: !hasMore,
    hasMore,
  };
}

async function queryLayerScopedRoleplayLoreEntrySearchPage(
  state: RoleplayRouteContext,
  query: NativeRoleplayLoreQuery,
  page: { limit: number; offset: number },
  recordIds: Set<string>,
): Promise<{
  entries: Record<string, unknown>[];
  total: number;
  totalExact: boolean;
  hasMore: boolean;
}> {
  const rawEntries = await queryAllRoleplayLoreEntriesForLayerScopedSearch(
    state,
    query,
  );
  const filteredEntries = rawEntries.filter((entry) =>
    recordIds.has(String(entry.record_id)),
  );
  const entries = filteredEntries.slice(page.offset, page.offset + page.limit);
  return {
    entries,
    total: filteredEntries.length,
    totalExact: true,
    hasMore: page.offset + entries.length < filteredEntries.length,
  };
}

async function queryAllRoleplayLoreEntriesForLayerScopedSearch(
  state: RoleplayRouteContext,
  baseQuery: NativeRoleplayLoreQuery,
): Promise<Record<string, unknown>[]> {
  const pageLimit = 1_000;
  const entries: Record<string, unknown>[] = [];
  for (let offset = 0; ; offset += pageLimit) {
    const pageEntries = await state.bridge.queryLoreEntries({
      ...baseQuery,
      page: {
        limit: pageLimit,
        offset,
      },
    });
    entries.push(...pageEntries);
    if (pageEntries.length < pageLimit) return entries;
  }
}

async function roleplayLoreSearchLayerScope(
  state: RoleplayRouteContext,
  input: {
    profileId?: string;
    chatId?: string;
    explicitLayerIds: readonly string[];
  },
): Promise<{
  source: "explicit" | "chat" | "profile" | "all";
  layerIds: string[];
  activeLayerIds: string[];
  layers: Record<string, unknown>[];
  recordIds?: Set<string>;
}> {
  let source: "explicit" | "chat" | "profile" | "all" = "all";
  let layerIds = [...input.explicitLayerIds];
  let activeLayerIds: string[] = [];

  if (layerIds.length > 0) {
    source = "explicit";
    activeLayerIds = [...layerIds];
  } else if (input.chatId) {
    source = "chat";
    const chatLayers = await state.bridge.getChatLayers(input.chatId);
    layerIds = chatLayers
      .filter((layer) => layer.enabled !== false)
      .sort((left, right) => Number(left.priority) - Number(right.priority))
      .map((layer) => String(layer.layer_id));
    activeLayerIds = [...layerIds];
  } else if (input.profileId) {
    source = "profile";
    const profileLayers = await state.bridge.listLoreLayers(input.profileId);
    layerIds = profileLayers.map((layer) => String(layer.layer_id));
    activeLayerIds = [...layerIds];
  }

  const layers = (
    await Promise.all(
      layerIds.map((layerId) =>
        state.bridge.getLoreLayer(layerId).catch(() => undefined),
      ),
    )
  ).filter((layer): layer is Record<string, unknown> => layer !== undefined);

  if (layerIds.length === 0) {
    return { source, layerIds, activeLayerIds, layers };
  }

  const recordIds = new Set<string>();
  await Promise.all(
    layerIds.map(async (layerId) => {
      const entries = await state.bridge.listEntriesByLayer(layerId);
      for (const entry of entries) {
        recordIds.add(String(entry.record_id));
      }
    }),
  );
  return { source, layerIds, activeLayerIds, layers, recordIds };
}

function roleplayLoreSearchLayerIds(params: URLSearchParams): string[] {
  const values = [
    ...params.getAll("layer_id"),
    ...params.getAll("layerId"),
    ...params.getAll("layer_ids"),
    ...params.getAll("layerIds"),
  ];
  return [
    ...new Set(
      values
        .flatMap((value) => value.split(","))
        .map((value) => value.trim())
        .filter((value) => value.length > 0),
    ),
  ];
}

function roleplayLoreSearchPage(params: URLSearchParams): {
  limit: number;
  offset: number;
} {
  const limit = integerQueryParam(params, "limit", 50);
  const offset = integerQueryParam(params, "offset", 0);
  return {
    limit: Math.min(Math.max(limit, 1), 200),
    offset: Math.max(offset, 0),
  };
}

function integerQueryParam(
  params: URLSearchParams,
  name: string,
  fallback: number,
): number {
  const raw = params.get(name);
  if (raw === null || raw.trim().length === 0) return fallback;
  const value = Number(raw);
  if (!Number.isInteger(value)) {
    throw new Error(`${name} must be an integer`);
  }
  return value;
}

function optionalBooleanQuery(
  params: URLSearchParams,
  name: string,
): boolean | undefined {
  const raw = params.get(name);
  if (raw === null || raw.trim().length === 0) return undefined;
  if (raw === "true" || raw === "1") return true;
  if (raw === "false" || raw === "0") return false;
  throw new Error(`${name} must be true or false`);
}

function browserSafeLoreEntry(
  entry: Record<string, unknown>,
): Record<string, unknown> {
  const loreControls = roleplayLoreControlsForEntry(entry);
  return {
    ...entry,
    primary_keys: loreControls.primary_keys,
    secondary_keys: loreControls.secondary_keys,
    enabled: loreControls.enabled,
    scan_depth: loreControls.scan_depth,
    insertion_position: loreControls.insertion_position,
    insertion_order: loreControls.insertion_order,
    probability: loreControls.probability,
    retrieval_role: loreControls.retrieval_role,
    lore_controls: loreControls,
    lore_control_support: roleplayLoreControlSupport(),
  };
}

function browserSafeLoreLayerEntry(
  entry: Record<string, unknown>,
): Record<string, unknown> {
  const record = recordBody(entry.record);
  const safeRecord = browserSafeLoreEntry(record);
  const layerControls = roleplayLoreControlsForEntry(record, {
    constant: optionalBoolean(entry.is_constant),
    insertionOrder: optionalNumber(entry.priority),
  });
  return {
    ...entry,
    record: safeRecord,
    constant: layerControls.constant,
    insertion_order: layerControls.insertion_order,
    lore_controls: layerControls,
    lore_control_support: roleplayLoreControlSupport(),
  };
}

function withEntryCount(
  layer: Record<string, unknown>,
  entryCount: number,
): Record<string, unknown> {
  return { ...layer, entry_count: entryCount, entryCount };
}

function roleplayLoreWriteFromBody(
  body: Record<string, unknown>,
  existing: Record<string, unknown> | undefined,
  now: string,
  options: {
    defaultRecordId?: string;
    forcedRecordId?: string;
    defaultSource: string;
    defaultDurabilityRationale: string;
  },
): NativeRoleplayLoreWrite {
  const recordId = requiredRouteString(
    options.forcedRecordId ??
      stringField(
        body,
        ["record_id", "recordId"],
        optionalString(existing?.record_id),
      ) ??
      options.defaultRecordId,
    "record_id",
  );
  const worldId = requiredRouteString(
    stringField(
      body,
      ["world_id", "worldId"],
      optionalString(existing?.world_id),
    ),
    "world_id",
  );
  const entityId = nullableStringField(
    body,
    ["entity_id", "entityId"],
    existing?.entity_id,
  );
  const title = requiredRouteString(
    stringField(body, ["title"], optionalString(existing?.title)),
    "title",
  );
  const loreBody = requiredRouteString(
    stringField(body, ["body"], optionalString(existing?.body)),
    "body",
  );
  const canonStatus = requiredRouteString(
    stringField(
      body,
      ["canon_status", "canonStatus"],
      optionalString(existing?.canon_status) ?? "draft",
    ),
    "canon_status",
  );
  const visibility = requiredRouteString(
    stringField(
      body,
      ["visibility"],
      optionalString(existing?.visibility) ?? "public",
    ),
    "visibility",
  );
  const content = roleplayLoreContentFromBody(body, existing, {
    worldId,
    entityId,
    title,
    body: loreBody,
    canonStatus,
    visibility,
  });
  return {
    record_id: recordId,
    world_id: worldId,
    entity_id: entityId,
    session_id: nullableStringField(
      body,
      ["session_id", "sessionId"],
      existing?.session_id,
    ),
    branch_id: nullableStringField(
      body,
      ["branch_id", "branchId"],
      existing?.branch_id,
    ),
    shape: roleplayLoreShapeFromBody(body, existing),
    canon_status: canonStatus,
    visibility,
    title,
    body: loreBody,
    content,
    evidence_refs: roleplayLoreEvidenceRefsFromBody(body, existing),
    source: stringField(body, ["source"], undefined) ?? options.defaultSource,
    confidence:
      optionalNumber(body.confidence) ??
      optionalNumber(existing?.confidence) ??
      1,
    durability_rationale:
      stringField(
        body,
        ["durability_rationale", "durabilityRationale"],
        optionalString(existing?.durability_rationale),
      ) ?? options.defaultDurabilityRationale,
    supersedes_record_id: nullableStringField(
      body,
      ["supersedes_record_id", "supersedesRecordId"],
      existing?.supersedes_record_id,
    ),
    now,
  };
}

function roleplayLoreShapeFromBody(
  body: Record<string, unknown>,
  existing: Record<string, unknown> | undefined,
): Record<string, unknown> {
  const bodyShape = recordBody(body.shape);
  const existingShape = recordBody(existing?.shape);
  return {
    shape_id:
      optionalString(bodyShape.shape_id) ??
      optionalString(bodyShape.shapeId) ??
      optionalString(body.shape_id) ??
      optionalString(body.shapeId) ??
      optionalString(existingShape.shape_id) ??
      "lore_entry",
    version:
      optionalNumber(bodyShape.version) ??
      optionalNumber(body.shape_version) ??
      optionalNumber(body.shapeVersion) ??
      optionalNumber(existingShape.version) ??
      1,
  };
}

function roleplayLoreContentFromBody(
  body: Record<string, unknown>,
  existing: Record<string, unknown> | undefined,
  fields: {
    worldId: string;
    entityId: string | null | undefined;
    title: string;
    body: string;
    canonStatus: string;
    visibility: string;
  },
): Record<string, unknown> {
  const rawContent = Object.hasOwn(body, "content")
    ? body.content
    : existing?.content;
  const content = isRecord(rawContent) ? { ...rawContent } : {};
  content.world_id = fields.worldId;
  if (fields.entityId === null || fields.entityId === undefined) {
    delete content.entity_id;
  } else {
    content.entity_id = fields.entityId;
  }
  content.title = fields.title;
  content.body = fields.body;
  content.canon_status = fields.canonStatus;
  content.visibility = fields.visibility;
  const controls = roleplayLoreControlsFromBody(body, content);
  if (controls !== undefined) {
    content.lore_controls = controls;
  }
  return content;
}

function roleplayLoreWriteBodyFromRequest(
  body: Record<string, unknown>,
): Record<string, unknown> {
  if (!Object.hasOwn(body, "write")) return body;
  const write = { ...recordBody(body.write) };
  for (const key of roleplayLoreControlRequestKeys()) {
    if (Object.hasOwn(write, key) || !Object.hasOwn(body, key)) continue;
    write[key] = body[key];
  }
  return write;
}

function roleplayLoreControlRequestKeys(): readonly string[] {
  return [
    "controls",
    "lore_controls",
    "loreControls",
    "primary_keys",
    "primaryKeys",
    "secondary_keys",
    "secondaryKeys",
    "enabled",
    "constant",
    "is_constant",
    "isConstant",
    "scan_depth",
    "scanDepth",
    "insertion_position",
    "insertionPosition",
    "insertion_order",
    "insertionOrder",
    "probability",
    "retrieval_role",
    "retrievalRole",
  ];
}

function roleplayLoreLayerEntryLinkFromBody(
  body: Record<string, unknown>,
  existing: Record<string, unknown>,
  now: string,
): Record<string, unknown> {
  const controls = roleplayLoreControlsFromBody(
    body,
    recordBody(existing.record),
  );
  return {
    layer_id: requiredRouteString(
      optionalString(existing.layer_id),
      "layer_id",
    ),
    record_id: requiredRouteString(
      optionalString(existing.record_id),
      "record_id",
    ),
    is_constant:
      optionalBoolean(body.is_constant) ??
      optionalBoolean(body.isConstant) ??
      optionalBoolean(body.constant) ??
      optionalBoolean(controls?.constant) ??
      optionalBoolean(existing.is_constant) ??
      false,
    priority: Math.trunc(
      optionalNumber(body.priority) ??
        optionalNumber(body.insertion_order) ??
        optionalNumber(body.insertionOrder) ??
        optionalNumber(controls?.insertion_order) ??
        optionalNumber(existing.priority) ??
        0,
    ),
    added_at: optionalString(existing.added_at) ?? now,
  };
}

function roleplayLoreControlsFromBody(
  body: Record<string, unknown>,
  existingSource: Record<string, unknown> | undefined,
): Record<string, unknown> | undefined {
  const existingContent = recordBody(
    Object.hasOwn(existingSource ?? {}, "content")
      ? existingSource?.content
      : existingSource,
  );
  const existingControls = recordBody(
    existingContent.lore_controls ?? existingContent.loreControls,
  );
  const explicitControls = Object.hasOwn(body, "lore_controls")
    ? recordBody(body.lore_controls)
    : Object.hasOwn(body, "loreControls")
      ? recordBody(body.loreControls)
      : Object.hasOwn(body, "controls")
        ? recordBody(body.controls)
        : undefined;
  const hasDirectControl = [
    "primary_keys",
    "primaryKeys",
    "secondary_keys",
    "secondaryKeys",
    "enabled",
    "constant",
    "is_constant",
    "isConstant",
    "scan_depth",
    "scanDepth",
    "insertion_position",
    "insertionPosition",
    "insertion_order",
    "insertionOrder",
    "probability",
    "retrieval_role",
    "retrievalRole",
  ].some((key) => Object.hasOwn(body, key));
  if (explicitControls === undefined && !hasDirectControl) {
    return Object.keys(existingControls).length === 0
      ? undefined
      : normalizeRoleplayLoreControls(existingControls);
  }
  return normalizeRoleplayLoreControls({
    ...existingControls,
    ...(explicitControls ?? {}),
    ...roleplayLoreDirectControlsPatch(body),
  });
}

function roleplayLoreDirectControlsPatch(
  body: Record<string, unknown>,
): Record<string, unknown> {
  const patch: Record<string, unknown> = {};
  copyFirstPresent(
    body,
    patch,
    ["primary_keys", "primaryKeys"],
    "primary_keys",
  );
  copyFirstPresent(
    body,
    patch,
    ["secondary_keys", "secondaryKeys"],
    "secondary_keys",
  );
  copyFirstPresent(body, patch, ["enabled"], "enabled");
  copyFirstPresent(
    body,
    patch,
    ["constant", "is_constant", "isConstant"],
    "constant",
  );
  copyFirstPresent(body, patch, ["scan_depth", "scanDepth"], "scan_depth");
  copyFirstPresent(
    body,
    patch,
    ["insertion_position", "insertionPosition"],
    "insertion_position",
  );
  copyFirstPresent(
    body,
    patch,
    ["insertion_order", "insertionOrder"],
    "insertion_order",
  );
  copyFirstPresent(body, patch, ["probability"], "probability");
  copyFirstPresent(
    body,
    patch,
    ["retrieval_role", "retrievalRole"],
    "retrieval_role",
  );
  return patch;
}

function copyFirstPresent(
  source: Record<string, unknown>,
  target: Record<string, unknown>,
  keys: readonly string[],
  targetKey: string,
): void {
  for (const key of keys) {
    if (!Object.hasOwn(source, key)) continue;
    target[targetKey] = source[key];
    return;
  }
}

function normalizeRoleplayLoreControls(
  raw: Record<string, unknown>,
): Record<string, unknown> {
  return {
    primary_keys: optionalControlStringList(
      raw.primary_keys ?? raw.primaryKeys,
      "primary_keys",
    ),
    secondary_keys: optionalControlStringList(
      raw.secondary_keys ?? raw.secondaryKeys,
      "secondary_keys",
    ),
    enabled: optionalBoolean(raw.enabled) ?? true,
    constant: optionalBoolean(raw.constant) ?? false,
    scan_depth: optionalBoundedInteger(raw.scan_depth ?? raw.scanDepth, {
      fieldName: "scan_depth",
      minimum: 0,
      maximum: 200,
      fallback: 4,
    }),
    insertion_position: optionalEnumString(
      raw.insertion_position ?? raw.insertionPosition,
      "insertion_position",
      [
        "before_history",
        "after_history",
        "before_author_note",
        "after_author_note",
        "system",
        "lore_block",
      ],
      "lore_block",
    ),
    insertion_order: optionalBoundedInteger(
      raw.insertion_order ?? raw.insertionOrder,
      {
        fieldName: "insertion_order",
        minimum: -1_000_000,
        maximum: 1_000_000,
        fallback: 0,
      },
    ),
    probability: optionalBoundedNumber(raw.probability, {
      fieldName: "probability",
      minimum: 0,
      maximum: 1,
      fallback: 1,
    }),
    retrieval_role: optionalEnumString(
      raw.retrieval_role ?? raw.retrievalRole,
      "retrieval_role",
      ["system", "user", "assistant", "narrator"],
      "system",
    ),
  };
}

function roleplayLoreControlsForEntry(
  entry: Record<string, unknown>,
  layerOverrides: { constant?: boolean; insertionOrder?: number } = {},
): Record<string, unknown> {
  const controls = normalizeRoleplayLoreControls(
    recordBody(recordBody(entry.content).lore_controls),
  );
  return {
    ...controls,
    constant: layerOverrides.constant ?? controls.constant,
    insertion_order: layerOverrides.insertionOrder ?? controls.insertion_order,
  };
}

function roleplayLoreControlSupport(): Record<string, string> {
  return {
    primary_keys: "stored_only",
    secondary_keys: "stored_only",
    enabled: "stored_only",
    scan_depth: "stored_only",
    insertion_position: "stored_only",
    probability: "stored_only",
    retrieval_role: "stored_only",
    constant: "layer_entry_recall",
    insertion_order: "layer_entry_priority_recall",
  };
}

function optionalControlStringList(
  value: unknown,
  fieldName: string,
): string[] {
  if (value === undefined) return [];
  if (typeof value === "string") {
    return value
      .split(",")
      .map((item) => item.trim())
      .filter((item) => item.length > 0);
  }
  if (!Array.isArray(value)) {
    throw new Error(`${fieldName} must be an array of strings`);
  }
  return value.map((item, index) =>
    requiredRouteString(optionalString(item), `${fieldName}[${index}]`),
  );
}

function optionalBoundedInteger(
  value: unknown,
  options: {
    fieldName: string;
    minimum: number;
    maximum: number;
    fallback: number;
  },
): number {
  if (value === undefined) return options.fallback;
  const parsed = optionalNumber(value);
  if (
    parsed === undefined ||
    !Number.isSafeInteger(parsed) ||
    parsed < options.minimum ||
    parsed > options.maximum
  ) {
    throw new Error(
      `${options.fieldName} must be an integer between ${options.minimum} and ${options.maximum}`,
    );
  }
  return parsed;
}

function optionalBoundedNumber(
  value: unknown,
  options: {
    fieldName: string;
    minimum: number;
    maximum: number;
    fallback: number;
  },
): number {
  if (value === undefined) return options.fallback;
  const parsed = optionalNumber(value);
  if (
    parsed === undefined ||
    parsed < options.minimum ||
    parsed > options.maximum
  ) {
    throw new Error(
      `${options.fieldName} must be a number between ${options.minimum} and ${options.maximum}`,
    );
  }
  return parsed;
}

function optionalEnumString(
  value: unknown,
  fieldName: string,
  allowed: readonly string[],
  fallback: string,
): string {
  const text = optionalString(value);
  if (text === undefined) return fallback;
  if (!allowed.includes(text)) {
    throw new Error(`${fieldName} must be one of ${allowed.join(", ")}`);
  }
  return text;
}

function roleplayLoreEvidenceRefsFromBody(
  body: Record<string, unknown>,
  existing: Record<string, unknown> | undefined,
): unknown[] {
  const value = Object.hasOwn(body, "evidence_refs")
    ? body.evidence_refs
    : Object.hasOwn(body, "evidenceRefs")
      ? body.evidenceRefs
      : existing?.evidence_refs;
  const refs = Array.isArray(value) ? value : [];
  return refs.length > 0
    ? refs
    : [
        {
          evidence_type: "ui",
          ref_id: "browser-admin",
          label: "Browser admin edit",
        },
      ];
}

function stringField(
  body: Record<string, unknown>,
  keys: readonly string[],
  fallback: string | undefined,
): string | undefined {
  for (const key of keys) {
    if (Object.hasOwn(body, key)) return optionalString(body[key]);
  }
  return fallback;
}

function stringListField(
  body: Record<string, unknown>,
  keys: readonly string[],
): string[] {
  for (const key of keys) {
    if (!Object.hasOwn(body, key)) continue;
    const value = body[key];
    if (Array.isArray(value)) {
      return value.map((item, index) =>
        requiredRouteString(optionalString(item), `${key}[${index}]`),
      );
    }
    const text = optionalString(value);
    return text === undefined ? [] : [text];
  }
  return [];
}

function nullableStringField(
  body: Record<string, unknown>,
  keys: readonly string[],
  fallback: unknown,
): string | null | undefined {
  for (const key of keys) {
    if (Object.hasOwn(body, key)) return optionalString(body[key]) ?? null;
  }
  return optionalString(fallback);
}

function roleplayLoreLayerWriteFromBody(
  body: Record<string, unknown>,
  pathProfileId: string | undefined,
  now: string,
): NativeRoleplayLoreLayerWrite {
  const layerId =
    optionalString(body.layer_id) ??
    optionalString(body.layerId) ??
    `layer-${randomBytes(6).toString("hex")}`;
  const profileId =
    pathProfileId ??
    optionalString(body.profile_id) ??
    optionalString(body.profileId);
  return {
    layer_id: requiredRouteString(layerId, "layer_id"),
    profile_id: requiredRouteString(profileId, "profile_id"),
    name: requiredRouteString(optionalString(body.name), "name"),
    description:
      optionalString(body.description) ??
      optionalString(body.summary) ??
      undefined,
    purpose:
      optionalString(body.purpose) ??
      optionalString(body.layerPurpose) ??
      "mixed",
    write_policy:
      optionalString(body.write_policy) ??
      optionalString(body.writePolicy) ??
      "manual",
    now,
  };
}

function roleplayLoreLayerUpdateFromBody(
  body: Record<string, unknown>,
  layerId: string,
  now: string,
): Record<string, unknown> {
  return {
    layer_id: layerId,
    ...(optionalString(body.name) === undefined
      ? {}
      : { name: optionalString(body.name) }),
    ...(Object.hasOwn(body, "description")
      ? { description: optionalString(body.description) ?? null }
      : {}),
    ...(optionalString(body.purpose) === undefined
      ? {}
      : { purpose: optionalString(body.purpose) }),
    ...(optionalString(body.write_policy) === undefined &&
    optionalString(body.writePolicy) === undefined
      ? {}
      : {
          write_policy:
            optionalString(body.write_policy) ??
            optionalString(body.writePolicy),
        }),
    now,
  };
}

function roleplayChatLayersWriteFromBody(
  body: Record<string, unknown>,
  now: string,
): RoleplayChatLayersBrowserWrite {
  const chatId = requiredRouteString(
    optionalString(body.chat_id) ??
      optionalString(body.chatId) ??
      optionalString(body.session_id) ??
      optionalString(body.sessionId),
    "chat_id",
  );
  const rawLayers = arrayValue(body.layers);
  const layerIds =
    rawLayers.length > 0
      ? rawLayers.map((layer, index) => {
          if (typeof layer === "string") {
            return { layer_id: layer, priority: index, enabled: true };
          }
          const record = recordBody(layer);
          return {
            layer_id: requiredRouteString(
              optionalString(record.layer_id) ?? optionalString(record.layerId),
              `layers[${index}].layer_id`,
            ),
            priority: optionalNumber(record.priority) ?? index,
            enabled: optionalBoolean(record.enabled) ?? true,
          };
        })
      : stringArray(body.layer_ids ?? body.layerIds, "layer_ids").map(
          (layerId, index) => ({
            layer_id: layerId,
            priority: index,
            enabled: true,
          }),
        );
  return { chat_id: chatId, layers: layerIds, now };
}

function requiredRouteString(
  value: string | undefined,
  fieldName: string,
): string {
  if (!value) throw new Error(`${fieldName} is required`);
  return value;
}

function requiredRouteBoolean(
  value: boolean | undefined,
  fieldName: string,
): boolean {
  if (value === undefined) throw new Error(`${fieldName} is required`);
  return value;
}

function requiredPositiveInteger(value: unknown, fieldName: string): number {
  const parsed = optionalNumber(value);
  if (parsed === undefined || !Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new Error(`${fieldName} must be a positive integer`);
  }
  return parsed;
}

function arrayValue(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function stringArray(value: unknown, fieldName: string): string[] {
  if (!Array.isArray(value)) throw new Error(`${fieldName} must be an array`);
  return value.map((item, index) =>
    requiredRouteString(optionalString(item), `${fieldName}[${index}]`),
  );
}

function optionalBoolean(value: unknown): boolean | undefined {
  return typeof value === "boolean" ? value : undefined;
}

function optionalNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value)
    ? value
    : undefined;
}

function recordBody(value: unknown): Record<string, unknown> {
  if (value === undefined || value === null) return {};
  if (!isRecord(value)) throw new Error("request body must be a JSON object");
  return value;
}

function requestId(request: IncomingMessage): string {
  const header = request.headers["x-request-id"];
  if (typeof header === "string" && header.length > 0) return header;
  if (Array.isArray(header) && header[0]) return header[0];
  return randomBytes(8).toString("hex");
}

async function readJsonBody(request: IncomingMessage): Promise<unknown> {
  const chunks: Buffer[] = [];
  for await (const chunk of request) {
    chunks.push(typeof chunk === "string" ? Buffer.from(chunk) : chunk);
  }
  if (chunks.length === 0) return {};
  const text = Buffer.concat(chunks).toString("utf8");
  if (text.trim().length === 0) return {};
  return JSON.parse(text) as unknown;
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message.length > 0
    ? error.message
    : fallback;
}
