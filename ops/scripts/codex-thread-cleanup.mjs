#!/usr/bin/env node

import { parseArgs } from "node:util";

const { values } = parseArgs({
  options: {
    "base-url": { type: "string" },
    "runtime-id": { type: "string" },
    apply: { type: "boolean", default: false },
  },
  strict: true,
});

const baseUrl = required(values["base-url"], "--base-url").replace(/\/$/, "");
const runtimeId = required(values["runtime-id"], "--runtime-id");
const apply = values.apply;
const headers = {
  accept: "application/json",
  ...(process.env.RUSTY_CREW_ADMIN_TOKEN
    ? { authorization: `Bearer ${process.env.RUSTY_CREW_ADMIN_TOKEN}` }
    : {}),
};

const certificationMarkers = [
  ["external_session_created_5675", /\bEXTERNAL_SESSION_CREATED_5675\b/],
  ["external_browser_create", /\bEXTERNAL_BROWSER_CREATE_OK\b/],
  ["external_service_live", /\bEXTERNAL_SERVICE_LIVE_OK\b/],
  ["external_peer_reply", /\bCODEX_CODEX_REPLY_OK\b/],
  ["view_projection", /\bRV_FRESH_DIFF_COMPLETE\b/],
  ["capability_edit", /\bCAPABILITY_EDIT_OK\b/],
  ["driver_smoke_recipient", /\bsmoke-recipient\b/],
  ["driver_echo_probe", /\brusty_crew\.echo_probe\b/],
];

const runtimeFleet = await getJson("/v1/external-runtimes");
const runtime = runtimeFleet.runtimes.find(
  (candidate) => candidate.runtimeId === runtimeId,
);
if (runtime === undefined) {
  throw new Error(`external runtime ${runtimeId} is not registered`);
}
const controller = runtimeFleet.controllers.find(
  (candidate) => candidate.runtimeId === runtimeId,
);
const [activeThreads, archivedThreads, bindingFleet, interactionFleet] =
  await Promise.all([
    listThreads(false),
    listThreads(true),
    getJson("/v1/external-bindings"),
    getJson("/v1/external-interactions"),
  ]);
const allNativeThreadIds = new Set(
  [...activeThreads, ...archivedThreads].map((thread) => thread.threadId),
);
const pendingThreadIds = new Set(
  interactionFleet.interactions.map(
    (interaction) => interaction.nativeThreadId,
  ),
);
const bindings = bindingFleet.bindings.filter(
  (binding) => binding.runtimeId === runtimeId,
);
const bindingsByThread = Map.groupBy(
  bindings.filter((binding) => binding.nativeThreadId != null),
  (binding) => binding.nativeThreadId,
);

const threads = activeThreads.map((thread) => {
  const matchedMarkers = certificationMarkers
    .filter(([, expression]) => expression.test(thread.preview))
    .map(([marker]) => marker);
  const reasons = [];
  if (thread.name != null) reasons.push("named_thread");
  if (thread.status === "active") reasons.push("native_thread_active");
  if (pendingThreadIds.has(thread.threadId))
    reasons.push("pending_interaction");
  if (matchedMarkers.length === 0)
    reasons.push("no_explicit_certification_marker");
  const eligible =
    matchedMarkers.length > 0 &&
    thread.name == null &&
    thread.status !== "active" &&
    !pendingThreadIds.has(thread.threadId);
  return {
    threadId: thread.threadId,
    sessionId: thread.sessionId,
    disposition: eligible ? "archive" : "preserve",
    matchedMarkers,
    preservationReasons: eligible ? [] : reasons,
    source: {
      name: thread.name,
      cwd: thread.cwd,
      createdAt: thread.createdAt,
      updatedAt: thread.updatedAt,
      status: thread.status,
      preview: thread.preview,
      agentNickname: thread.agentNickname,
      agentRole: thread.agentRole,
    },
    bindings: (bindingsByThread.get(thread.threadId) ?? []).map(projectBinding),
  };
});

const resumeFailures = new Map(
  (controller?.bindingResumeFailures ?? []).map((failure) => [
    failure.bindingId,
    failure,
  ]),
);
const orphanBindings = bindings
  .filter(
    (binding) =>
      binding.nativeThreadId != null &&
      !allNativeThreadIds.has(binding.nativeThreadId),
  )
  .map((binding) => {
    const failure = resumeFailures.get(binding.bindingId);
    const exactMissingHistoryFailure =
      failure?.nativeThreadId === binding.nativeThreadId &&
      failure.reason.includes("no rollout found for thread id");
    const pendingInteraction = pendingThreadIds.has(binding.nativeThreadId);
    const alreadyArchived = binding.status === "archived";
    return {
      ...projectBinding(binding),
      disposition:
        exactMissingHistoryFailure && !pendingInteraction && !alreadyArchived
          ? "archive_binding"
          : "preserve",
      preservationReasons:
        exactMissingHistoryFailure && !pendingInteraction && !alreadyArchived
          ? []
          : [
              ...(alreadyArchived ? ["binding_already_archived"] : []),
              ...(failure === undefined
                ? ["no_controller_resume_failure"]
                : []),
              ...(pendingInteraction ? ["pending_interaction"] : []),
            ],
      resumeFailure: failure ?? null,
    };
  });

const manifest = {
  schemaVersion: 1,
  mode: apply ? "apply" : "dry_run",
  baseUrl,
  runtime: {
    runtimeId,
    codexHomeRef: runtime.codexHomeRef ?? null,
    observedState: runtime.observedState,
    driverState: controller?.driverState ?? null,
  },
  countsBefore: {
    defaultThreads: activeThreads.length,
    archivedThreads: archivedThreads.length,
    bindings: bindings.length,
    pendingInteractions: interactionFleet.interactions.length,
    threadArchiveCandidates: threads.filter(
      (thread) => thread.disposition === "archive",
    ).length,
    orphanBindingArchiveCandidates: orphanBindings.filter(
      (binding) => binding.disposition === "archive_binding",
    ).length,
  },
  threads,
  orphanBindings,
  applyResults: [],
};

if (apply) {
  for (const thread of threads.filter(
    (candidate) => candidate.disposition === "archive",
  )) {
    manifest.applyResults.push(
      await postJson(
        `/v1/external-runtimes/${encodeURIComponent(runtimeId)}/threads/${encodeURIComponent(thread.threadId)}/archive`,
      ),
    );
  }
  for (const binding of orphanBindings.filter(
    (candidate) => candidate.disposition === "archive_binding",
  )) {
    const controlId = `codex-thread-cleanup-v2:${runtimeId}:${binding.bindingId}:${binding.revision}`;
    manifest.applyResults.push(
      await postJson(
        `/v1/external-bindings/${encodeURIComponent(binding.bindingId)}/controls`,
        {
          controlId,
          kind: "archive_binding",
          expectedBindingRevision: binding.revision,
          idempotencyKey: controlId,
        },
      ),
    );
  }
}

process.stdout.write(`${JSON.stringify(manifest, null, 2)}\n`);

async function listThreads(archived) {
  const result = [];
  let cursor;
  const seen = new Set();
  for (;;) {
    const url = new URL(
      `/v1/external-runtimes/${encodeURIComponent(runtimeId)}/threads`,
      `${baseUrl}/`,
    );
    url.searchParams.set("limit", "100");
    url.searchParams.set("archived", String(archived));
    if (cursor !== undefined) url.searchParams.set("cursor", cursor);
    const page = await fetchJson(url);
    result.push(...page.items);
    if (page.nextCursor == null) return result;
    if (seen.has(page.nextCursor)) {
      throw new Error("native thread pagination returned a repeated cursor");
    }
    seen.add(page.nextCursor);
    cursor = page.nextCursor;
  }
}

async function getJson(path) {
  return fetchJson(new URL(path, `${baseUrl}/`));
}

async function postJson(path, body) {
  const response = await fetch(new URL(path, `${baseUrl}/`), {
    method: "POST",
    headers: {
      ...headers,
      ...(body === undefined ? {} : { "content-type": "application/json" }),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  const envelope = await response.json();
  if (!response.ok || envelope.ok !== true) {
    throw new Error(
      `POST ${path} failed (${response.status}): ${JSON.stringify(envelope)}`,
    );
  }
  return envelope.data;
}

async function fetchJson(url) {
  const response = await fetch(url, { headers });
  const envelope = await response.json();
  if (!response.ok || envelope.ok !== true) {
    throw new Error(
      `GET ${url.pathname} failed (${response.status}): ${JSON.stringify(envelope)}`,
    );
  }
  return envelope.data;
}

function projectBinding(binding) {
  return {
    bindingId: binding.bindingId,
    nativeThreadId: binding.nativeThreadId,
    status: binding.status,
    revision: binding.revision,
    agentId: binding.agentId,
    sessionId: binding.sessionId,
    taskRef: binding.taskRef,
    cwd: binding.cwd,
  };
}

function required(value, label) {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${label} is required`);
  }
  return value;
}
