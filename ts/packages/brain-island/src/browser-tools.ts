import type {
  AgentTool as PiAgentTool,
  AgentToolResult,
} from "@earendil-works/pi-agent-core";
import type { SessionState } from "@rusty-crew/contracts";
import { Type, type Static } from "typebox";
import {
  BrowserSessionManager,
  type BrowserRefEntry,
  type CdpConnection,
} from "./browser-session-manager.js";
import type { BrainToolResolver } from "./tool-session-selection.js";
import { assertSafePublicUrl, type ResolveHostAddresses } from "./web-tools.js";

export interface BrowserToolContext {
  manager: BrowserSessionManager;
  session: Pick<SessionState, "sessionId" | "agentId" | "profileId">;
  pageLoadTimeoutMs?: number;
  allowPrivateNet?: boolean;
  resolveHostAddresses?: ResolveHostAddresses;
  screenshotStore?: BrowserScreenshotStore;
  maxScreenshotBytes?: number;
}

export interface BrowserToolResolverContext {
  manager: BrowserSessionManager;
  pageLoadTimeoutMs?: number;
  allowPrivateNet?: boolean;
  resolveHostAddresses?: ResolveHostAddresses;
  screenshotStore?: BrowserScreenshotStore;
  maxScreenshotBytes?: number;
}

export interface BrowserSnapshotDetails {
  ok: boolean;
  title: string;
  url: string;
  generation: number;
  refCount: number;
  refs: readonly BrowserRefEntry[];
}

export interface BrowserConsoleDetails {
  ok: boolean;
  expression: BrowserConsoleExpression;
  result: unknown;
}

export interface BrowserScreenshotArtifact {
  ref: string;
  mediaType: "image/png";
  byteLength: number;
  createdAt: string;
}

export interface BrowserScreenshotStore {
  store(input: {
    sessionId: string;
    mediaType: "image/png";
    data: Uint8Array;
  }): Promise<BrowserScreenshotArtifact> | BrowserScreenshotArtifact;
}

export interface BrowserVisionDetails {
  ok: boolean;
  artifact: BrowserScreenshotArtifact;
}

export interface BrowserActionDetails {
  ok: boolean;
  action: "navigate" | "click" | "type" | "scroll" | "back" | "press";
  ref?: string;
  target?: string;
  url?: string;
  direction?: "up" | "down";
  key?: string;
  textLength?: number;
}

export type BrowserConsoleExpression =
  | "document.title"
  | "location.href"
  | "document.readyState";

const browserSnapshotParameters = Type.Object({});
const browserNavigateParameters = Type.Object({
  url: Type.String({ minLength: 1 }),
});
const browserConsoleParameters = Type.Object({
  expression: Type.Optional(
    Type.Union([
      Type.Literal("document.title"),
      Type.Literal("location.href"),
      Type.Literal("document.readyState"),
    ]),
  ),
});
const browserRefParameters = Type.Object({
  ref: Type.String({ minLength: 1 }),
});
const browserTypeParameters = Type.Object({
  ref: Type.String({ minLength: 1 }),
  text: Type.String(),
});
const browserScrollParameters = Type.Object({
  direction: Type.Union([Type.Literal("up"), Type.Literal("down")]),
});
const browserBackParameters = Type.Object({});
const browserPressParameters = Type.Object({
  key: Type.String({ minLength: 1 }),
});
const browserVisionParameters = Type.Object({});

type BrowserConsoleParams = Static<typeof browserConsoleParameters>;
type BrowserNavigateParams = Static<typeof browserNavigateParameters>;
type BrowserRefParams = Static<typeof browserRefParameters>;
type BrowserTypeParams = Static<typeof browserTypeParameters>;
type BrowserScrollParams = Static<typeof browserScrollParameters>;
type BrowserPressParams = Static<typeof browserPressParameters>;

const defaultMaxScreenshotBytes = 2 * 1024 * 1024;

export class MemoryBrowserScreenshotStore implements BrowserScreenshotStore {
  readonly #artifacts = new Map<string, Uint8Array>();
  #nextId = 1;

  store(input: {
    sessionId: string;
    mediaType: "image/png";
    data: Uint8Array;
  }): BrowserScreenshotArtifact {
    const ref = `browser-screenshot:${input.sessionId}:${this.#nextId}`;
    this.#nextId += 1;
    this.#artifacts.set(ref, input.data);
    return {
      ref,
      mediaType: input.mediaType,
      byteLength: input.data.byteLength,
      createdAt: new Date().toISOString(),
    };
  }

  get(ref: string): Uint8Array | undefined {
    return this.#artifacts.get(ref);
  }
}

export function createBrowserToolResolver(
  context: BrowserToolResolverContext,
): BrainToolResolver {
  return ({ wake }) =>
    resolveBrowserTools({
      manager: context.manager,
      session: wake.state.session,
      pageLoadTimeoutMs: context.pageLoadTimeoutMs,
      allowPrivateNet: context.allowPrivateNet,
      resolveHostAddresses: context.resolveHostAddresses,
      screenshotStore: context.screenshotStore,
      maxScreenshotBytes: context.maxScreenshotBytes,
    });
}

export function resolveBrowserTools(
  context: BrowserToolContext,
): PiAgentTool[] {
  return [
    browserNavigateTool(context),
    browserSnapshotTool(context),
    browserClickTool(context),
    browserTypeTool(context),
    browserScrollTool(context),
    browserBackTool(context),
    browserPressTool(context),
    browserConsoleTool(context),
    browserVisionTool(context),
  ];
}

export function browserNavigateTool(
  context: BrowserToolContext,
): PiAgentTool<typeof browserNavigateParameters, BrowserActionDetails> {
  return {
    name: "browser_navigate",
    label: "Browser navigate",
    description:
      "Navigate the session-scoped browser to an allowed public HTTP(S) URL.",
    parameters: browserNavigateParameters,
    execute: async (_toolCallId, params: BrowserNavigateParams, signal) => {
      await assertSafePublicUrl(params.url, {
        allowPrivateNet: context.allowPrivateNet,
        resolveHostAddresses: context.resolveHostAddresses,
      });
      const handle = await context.manager.open(context.session, signal);
      await handle.cdp.call("Page.navigate", { url: params.url });
      await handle.cdp
        .call("Page.loadEventFired", {}, context.pageLoadTimeoutMs ?? 8_000)
        .catch(() => undefined);
      context.manager.recordNavigation(context.session.sessionId, params.url);
      const details = {
        ok: true,
        action: "navigate",
        url: params.url,
      } satisfies BrowserActionDetails;
      return textResult(`navigated ${params.url}`, details);
    },
  };
}

export function browserSnapshotTool(
  context: BrowserToolContext,
): PiAgentTool<typeof browserSnapshotParameters, BrowserSnapshotDetails> {
  return {
    name: "browser_snapshot",
    label: "Browser snapshot",
    description:
      "Return a bounded accessibility and interactive-element snapshot for the session-scoped browser page.",
    parameters: browserSnapshotParameters,
    execute: async (_toolCallId, _params, signal) => {
      const handle = await context.manager.open(context.session, signal);
      const title = await evalJson<string>(handle.cdp, "document.title");
      const url = await evalJson<string>(handle.cdp, "location.href");
      const refs = await evalJson<readonly DomRef[]>(handle.cdp, domRefScript);
      const axTree = await handle.cdp.call("Accessibility.getFullAXTree", {});
      const snapshot = context.manager.storeRefs(
        context.session.sessionId,
        refs.map((ref, index) => ({
          ref: `@e${index}`,
          target: ref.selector,
          role: ref.role,
          name: ref.name,
        })),
      );
      const details = {
        ok: true,
        title,
        url,
        generation: snapshot.generation,
        refCount: snapshot.refs.length,
        refs: snapshot.refs,
      } satisfies BrowserSnapshotDetails;
      return textResult(renderSnapshotText(details, axTree), details);
    },
  };
}

export function browserClickTool(
  context: BrowserToolContext,
): PiAgentTool<typeof browserRefParameters, BrowserActionDetails> {
  return {
    name: "browser_click",
    label: "Browser click",
    description: "Click a ref from the current browser snapshot.",
    parameters: browserRefParameters,
    execute: async (_toolCallId, params: BrowserRefParams, signal) => {
      const handle = await context.manager.open(context.session, signal);
      const target = currentRefTarget(context, params.ref);
      await handle.cdp.call("Runtime.evaluate", {
        expression: clickScript(target),
        awaitPromise: true,
      });
      context.manager.invalidateRefs(context.session.sessionId);
      const details = {
        ok: true,
        action: "click",
        ref: params.ref,
        target,
      } satisfies BrowserActionDetails;
      return textResult(`clicked ${params.ref}`, details);
    },
  };
}

export function browserTypeTool(
  context: BrowserToolContext,
): PiAgentTool<typeof browserTypeParameters, BrowserActionDetails> {
  return {
    name: "browser_type",
    label: "Browser type",
    description:
      "Type bounded text into a ref from the current browser snapshot.",
    parameters: browserTypeParameters,
    execute: async (_toolCallId, params: BrowserTypeParams, signal) => {
      const handle = await context.manager.open(context.session, signal);
      const target = currentRefTarget(context, params.ref);
      await handle.cdp.call("Runtime.evaluate", {
        expression: typeScript(target, params.text),
        awaitPromise: true,
      });
      context.manager.invalidateRefs(context.session.sessionId);
      const details = {
        ok: true,
        action: "type",
        ref: params.ref,
        target,
        textLength: params.text.length,
      } satisfies BrowserActionDetails;
      return textResult(`typed into ${params.ref}`, details);
    },
  };
}

export function browserScrollTool(
  context: BrowserToolContext,
): PiAgentTool<typeof browserScrollParameters, BrowserActionDetails> {
  return {
    name: "browser_scroll",
    label: "Browser scroll",
    description: "Scroll the current browser page up or down.",
    parameters: browserScrollParameters,
    execute: async (_toolCallId, params: BrowserScrollParams, signal) => {
      const handle = await context.manager.open(context.session, signal);
      const distance = params.direction === "up" ? -700 : 700;
      await handle.cdp.call("Runtime.evaluate", {
        expression: `window.scrollBy(0, ${distance})`,
      });
      context.manager.invalidateRefs(context.session.sessionId);
      const details = {
        ok: true,
        action: "scroll",
        direction: params.direction,
      } satisfies BrowserActionDetails;
      return textResult(`scrolled ${params.direction}`, details);
    },
  };
}

export function browserBackTool(
  context: BrowserToolContext,
): PiAgentTool<typeof browserBackParameters, BrowserActionDetails> {
  return {
    name: "browser_back",
    label: "Browser back",
    description: "Navigate back in the current browser history.",
    parameters: browserBackParameters,
    execute: async (_toolCallId, _params, signal) => {
      const handle = await context.manager.open(context.session, signal);
      await handle.cdp.call("Runtime.evaluate", {
        expression: "history.back()",
      });
      context.manager.invalidateRefs(context.session.sessionId);
      const details = {
        ok: true,
        action: "back",
      } satisfies BrowserActionDetails;
      return textResult("back", details);
    },
  };
}

export function browserPressTool(
  context: BrowserToolContext,
): PiAgentTool<typeof browserPressParameters, BrowserActionDetails> {
  return {
    name: "browser_press",
    label: "Browser press",
    description: "Press a bounded keyboard key on the current browser page.",
    parameters: browserPressParameters,
    execute: async (_toolCallId, params: BrowserPressParams, signal) => {
      const handle = await context.manager.open(context.session, signal);
      await handle.cdp.call("Input.dispatchKeyEvent", {
        type: "keyDown",
        key: params.key,
      });
      await handle.cdp.call("Input.dispatchKeyEvent", {
        type: "keyUp",
        key: params.key,
      });
      context.manager.invalidateRefs(context.session.sessionId);
      const details = {
        ok: true,
        action: "press",
        key: params.key,
      } satisfies BrowserActionDetails;
      return textResult(`pressed ${params.key}`, details);
    },
  };
}

export function browserConsoleTool(
  context: BrowserToolContext,
): PiAgentTool<typeof browserConsoleParameters, BrowserConsoleDetails> {
  return {
    name: "browser_console",
    label: "Browser console",
    description:
      "Run one bounded page diagnostic expression and return a JSON-serializable result.",
    parameters: browserConsoleParameters,
    execute: async (_toolCallId, params: BrowserConsoleParams, signal) => {
      const handle = await context.manager.open(context.session, signal);
      const expression = params.expression ?? "document.title";
      const result = await evalJson<unknown>(handle.cdp, expression);
      const details = {
        ok: true,
        expression,
        result,
      } satisfies BrowserConsoleDetails;
      context.manager.recordConsole(
        context.session.sessionId,
        `${expression}: ${JSON.stringify(result)}`,
      );
      return textResult(JSON.stringify(details, null, 2), details);
    },
  };
}

export function browserVisionTool(
  context: BrowserToolContext,
): PiAgentTool<typeof browserVisionParameters, BrowserVisionDetails> {
  return {
    name: "browser_vision",
    label: "Browser vision capture",
    description:
      "Capture a bounded PNG screenshot and return an artifact reference for later vision analysis.",
    parameters: browserVisionParameters,
    execute: async (_toolCallId, _params, signal) => {
      const handle = await context.manager.open(context.session, signal);
      const result = (await handle.cdp.call("Page.captureScreenshot", {
        format: "png",
      })) as { data?: string };
      const data = Buffer.from(result.data ?? "", "base64");
      const maxBytes = context.maxScreenshotBytes ?? defaultMaxScreenshotBytes;
      if (data.byteLength > maxBytes) {
        throw new Error("browser screenshot exceeds configured byte limit");
      }
      if (!context.screenshotStore) {
        throw new Error("browser screenshot artifact store is not configured");
      }
      const artifact = await context.screenshotStore.store({
        sessionId: context.session.sessionId,
        mediaType: "image/png",
        data,
      });
      const details = {
        ok: true,
        artifact,
      } satisfies BrowserVisionDetails;
      return textResult(JSON.stringify(details, null, 2), details);
    },
  };
}

interface DomRef {
  selector: string;
  role: string;
  name: string;
}

const domRefScript = `Array.from(document.querySelectorAll('a,button,input,textarea,select,[role="button"],[tabindex]')).slice(0,80).map((el,i)=>{ if(!el.dataset.rustyCrewRef) el.dataset.rustyCrewRef='e'+i; return {selector:'[data-rusty-crew-ref="'+el.dataset.rustyCrewRef+'"]', role:el.getAttribute('role')||el.tagName.toLowerCase(), name:(el.innerText||el.value||el.getAttribute('aria-label')||el.getAttribute('title')||el.href||'').trim().slice(0,120)} })`;

function currentRefTarget(context: BrowserToolContext, ref: string): string {
  const snapshot = context.manager.snapshot(context.session.sessionId);
  if (!snapshot) {
    throw new Error("browser snapshot is required before using refs");
  }
  const resolved = context.manager.resolveRef(
    context.session.sessionId,
    snapshot.generation,
    ref,
  );
  if (!resolved) {
    throw new Error(
      `stale or unknown browser ref ${ref}; call browser_snapshot`,
    );
  }
  return resolved.target;
}

function clickScript(selector: string): string {
  return `document.querySelector(${JSON.stringify(selector)})?.click()`;
}

function typeScript(selector: string, text: string): string {
  return `(() => { const el = document.querySelector(${JSON.stringify(
    selector,
  )}); if (!el) throw new Error('missing element'); el.focus(); if ('value' in el) el.value = ${JSON.stringify(
    text,
  )}; else el.textContent = ${JSON.stringify(
    text,
  )}; el.dispatchEvent(new Event('input', {bubbles:true})); })()`;
}

async function evalJson<T>(cdp: CdpConnection, expression: string): Promise<T> {
  const result = (await cdp.call("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  })) as { result?: { value?: T } };
  return result.result?.value as T;
}

function renderSnapshotText(
  details: BrowserSnapshotDetails,
  axTree: unknown,
): string {
  const lines = [
    `title: ${details.title}`,
    `url: ${details.url}`,
    `generation: ${details.generation}`,
    "",
    "interactive elements:",
    ...details.refs.map((ref) =>
      `[${ref.ref}] ${ref.role ?? "element"} ${ref.name ?? ""}`.trim(),
    ),
    "",
    "accessibility tree excerpt:",
    ...axTreeLines(axTree).slice(0, 80),
  ];
  return lines.join("\n");
}

function axTreeLines(value: unknown): string[] {
  const nodes =
    (
      value as {
        nodes?: readonly {
          role?: { value?: string };
          name?: { value?: string };
        }[];
      }
    ).nodes ?? [];
  return nodes
    .map((node) => `${node.role?.value ?? "node"}: ${node.name?.value ?? ""}`)
    .filter((line) => line.trim().length > 1);
}

function textResult<TDetails extends Record<string, unknown>>(
  text: string,
  details: TDetails,
): AgentToolResult<TDetails> {
  return {
    content: [{ type: "text", text }],
    details,
  };
}
