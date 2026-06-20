import assert from "node:assert/strict";
import type {
  AgentId,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import {
  assertSafePublicUrl,
  resolveToolSession,
  resolveWebTools,
  webExtractTool,
} from "./index.js";
import type {
  BrainWakeInput,
  ResolvedAddress,
  ResolveHostAddresses,
} from "./index.js";

const resolvedHosts: Record<string, readonly ResolvedAddress[]> = {
  "public.test": [{ address: "93.184.216.34", family: 4 }],
  "private.test": [{ address: "10.0.0.7", family: 4 }],
};

const resolveHostAddresses: ResolveHostAddresses = async (hostname) =>
  resolvedHosts[hostname] ?? [];

const fetchCalls: string[] = [];
const fetchImpl: typeof fetch = async (input) => {
  const url = String(input);
  fetchCalls.push(url);
  if (url === "https://public.test/page") {
    return new Response(
      "<html><title>Public Page</title><body><p>Hello <b>world</b>.</p></body></html>",
      {
        status: 200,
        headers: { "content-type": "text/html; charset=utf-8" },
      },
    );
  }
  if (url === "https://public.test/redirect-private") {
    return new Response("", {
      status: 302,
      headers: { location: "http://127.0.0.1/private" },
    });
  }
  return new Response("missing", { status: 404 });
};

const tool = webExtractTool({
  fetchImpl,
  resolveHostAddresses,
  maxExtractChars: 20,
  maxExtractBytes: 200,
});

const success = await tool.execute("extract-success", {
  urls: ["https://public.test/page"],
});
assert.equal(success.details.ok, true);
assert.equal(success.details.results[0]?.title, "Public Page");
assert.equal(success.details.results[0]?.content, "Public PageHello wor");
assert.equal(success.details.results[0]?.truncated, true);

const mixed = await tool.execute("extract-mixed", {
  urls: ["https://public.test/page", "http://localhost/private"],
});
assert.equal(mixed.details.ok, false);
assert.equal(mixed.details.results[0]?.title, "Public Page");
assert.equal(mixed.details.results[1]?.reasonCode, "private_network");

const denied = await tool.execute("extract-denied", {
  urls: [
    "http://localhost/private",
    "http://service.localhost/private",
    "https://private.test/data",
    "https://public.test/redirect-private",
    "file:///etc/passwd",
  ],
});
assert.equal(denied.details.ok, false);
assert.deepEqual(
  denied.details.results.map((result) => result.reasonCode),
  [
    "private_network",
    "private_network",
    "private_network",
    "private_network",
    "unsupported_scheme",
  ],
);
assert.equal(fetchCalls.includes("http://127.0.0.1/private"), false);

const redirectChain = await webExtractTool({
  fetchImpl: async (input) => {
    const url = String(input);
    if (url === "https://public.test/a") {
      return new Response("", {
        status: 302,
        headers: { location: "https://public.test/b" },
      });
    }
    if (url === "https://public.test/b") {
      return new Response("", {
        status: 302,
        headers: { location: "https://public.test/c" },
      });
    }
    return new Response("<title>redirect ok</title>", {
      headers: { "content-type": "text/html" },
    });
  },
  resolveHostAddresses,
  maxRedirects: 1,
}).execute("extract-redirect-chain", {
  urls: ["https://public.test/a"],
});
assert.equal(
  redirectChain.details.results[0]?.reasonCode,
  "too_many_redirects",
);

const binaryDenied = await webExtractTool({
  fetchImpl: async () =>
    new Response(new Uint8Array([1, 2, 3]), {
      headers: { "content-type": "application/octet-stream" },
    }),
  resolveHostAddresses,
}).execute("extract-binary", {
  urls: ["https://public.test/binary"],
});
assert.equal(
  binaryDenied.details.results[0]?.reasonCode,
  "unsupported_content_type",
);

const allowedPrivate = await webExtractTool({
  fetchImpl: async () =>
    new Response("<title>Private OK</title><body>allowed</body>", {
      status: 200,
      headers: { "content-type": "text/html" },
    }),
  resolveHostAddresses,
  allowPrivateNet: true,
}).execute("extract-private", {
  urls: ["https://private.test/data"],
});
assert.equal(allowedPrivate.details.ok, true);
assert.equal(allowedPrivate.details.allowPrivateNet, true);

await assertSafePublicUrl("https://public.test/page", {
  resolveHostAddresses,
});
await assert.rejects(
  () =>
    assertSafePublicUrl("http://10.0.0.1/private", {
      resolveHostAddresses,
    }),
  /private-network URLs are blocked/,
);
await assert.rejects(
  () =>
    assertSafePublicUrl("http://172.16.0.1/private", {
      resolveHostAddresses,
    }),
  /private-network URLs are blocked/,
);
await assert.rejects(
  () =>
    assertSafePublicUrl("http://192.168.1.10/private", {
      resolveHostAddresses,
    }),
  /private-network URLs are blocked/,
);
await assert.rejects(
  () =>
    assertSafePublicUrl("http://100.64.0.1/private", {
      resolveHostAddresses,
    }),
  /private-network URLs are blocked/,
);
await assert.rejects(
  () =>
    assertSafePublicUrl("http://[::1]/private", {
      resolveHostAddresses,
    }),
  /private-network URLs are blocked/,
);
await assert.rejects(
  () =>
    assertSafePublicUrl("http://[fe80::1]/private", {
      resolveHostAddresses,
    }),
  /private-network URLs are blocked/,
);
await assert.rejects(
  () =>
    assertSafePublicUrl("http://[fc00::1]/private", {
      resolveHostAddresses,
    }),
  /private-network URLs are blocked/,
);
await assert.rejects(
  () =>
    assertSafePublicUrl("http://[::ffff:127.0.0.1]/private", {
      resolveHostAddresses,
    }),
  /private-network URLs are blocked/,
);
await assert.rejects(
  () =>
    assertSafePublicUrl("http://2130706433/private", {
      resolveHostAddresses,
    }),
  /private-network URLs are blocked/,
);
await assert.rejects(
  () =>
    assertSafePublicUrl("https://public.test:8443/page", {
      resolveHostAddresses,
    }),
  /non-standard URL ports are blocked/,
);

const toolSession = resolveToolSession({
  wake: wakeWithWebExtract(),
  resolveTools: () =>
    resolveWebTools({
      fetchImpl,
      resolveHostAddresses,
    }),
});
assert.deepEqual(
  toolSession.tools.map((resolvedTool) => resolvedTool.name),
  ["web_extract"],
);
assert.equal(toolSession.items[0]?.status, "callable");

console.log(
  JSON.stringify(
    {
      successTitle: success.details.results[0]?.title,
      deniedReasons: denied.details.results.map((result) => result.reasonCode),
      selectedTools: toolSession.tools.map((resolvedTool) => resolvedTool.name),
      fetchCalls,
    },
    null,
    2,
  ),
);

function wakeWithWebExtract(): BrainWakeInput {
  return {
    wakeId: "wake-web-extract",
    sessionId: "session-web-extract" as SessionId,
    state: {
      session: {
        handle: 2 as SessionHandle,
        sessionId: "session-web-extract" as SessionId,
        agentId: "agent-web-extract" as AgentId,
        profileId: "profile-web-extract" as ProfileId,
        kind: "full",
        status: "active",
        brainTurnCount: 0,
        createdAt: "2026-06-20T00:00:00.000Z",
        lastActiveAt: "2026-06-20T00:00:00.000Z",
        resourceLimits: {},
        toolProfile: {
          tools: [
            {
              name: "web_extract",
              description:
                "Fetch and extract bounded public page text with SSRF guardrails.",
            },
          ],
        },
      },
      pendingMessages: [],
      recentEvents: [],
      childCompletions: [],
      fanOutGroups: [],
      deltaPolicy: {
        mode: "frozen_snapshot_next_wake",
        queueOwner: "body",
        queuedMessageTtlMs: 60_000,
        maxQueuedMessages: 8,
      },
    },
    systemPrompt: "",
    roleAssembly: {
      instructions: "Use web_extract when needed.",
    },
  };
}
