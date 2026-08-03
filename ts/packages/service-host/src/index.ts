import {
  createServer,
  type IncomingMessage,
  type Server,
  type ServerResponse,
} from "node:http";

import {
  createRustyCrewServiceApp,
  loadRustyCrewServiceConfig,
  type ServiceAdapterFactories,
  type RustyCrewServiceApp,
  type RustyCrewServiceAppOptions,
} from "@rusty-crew/brain-island";
import {
  createDenMemoryClient,
  ReviewGitHubGateEventConsumer,
  createDenSuccessorGatewayClient,
  dispatchChannelMessageProjection,
  ingestChannelInboundMessage,
  projectAgentMessageToChannel,
  resolveDenConversationChannels,
} from "@rusty-crew/adapter-den";
import {
  createSimulatedMcpTransportFactory,
  McpSurfaceManager,
} from "@rusty-crew/adapter-mcp";
import {
  createTelegramAdapterRegistration,
  createTelegramBotApiHttpClient,
  FileTelegramUpdateOffsetStore,
  TelegramChannelConnector,
} from "@rusty-crew/adapter-telegram";
import type { AdapterId, EngineHandle } from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";

import { handleHostShellRequest, requestId } from "./host-shell-routes.js";
import { hostFailure, writeHostRouteResult } from "./host-route-results.js";
import {
  startServiceHostBackgroundLoopTimers,
  type ServiceHostBackgroundLoopController,
} from "./background-loop-timers.js";
import {
  assertServiceHostStorageBootReady,
  preflightServiceHostStorageBoot,
} from "./storage-preflight.js";

export {
  createSystemdNotifier,
  localHealthBaseUrl,
  systemdNotifyArguments,
  watchdogIntervalFromUsec,
  type SystemdNotifier,
  type SystemdNotifierOptions,
} from "./systemd-notify.js";
export {
  startServiceHostBackgroundLoopTimers,
  type ServiceHostBackgroundLoopController,
} from "./background-loop-timers.js";
export {
  assertServiceHostStorageBootReady,
  preflightServiceHostStorageBoot,
  type ServiceHostStorageBootPreflight,
} from "./storage-preflight.js";

export interface RustyCrewServiceHostOptions extends Omit<
  RustyCrewServiceAppOptions,
  "adapterFactories"
> {
  adapterFactories?: Partial<ServiceAdapterFactories>;
}

export interface RustyCrewServiceHost {
  readonly app: RustyCrewServiceApp;
  readonly config: RustyCrewServiceApp["config"];
  readonly bridge: NativeBridgeModule;
  readonly engine: EngineHandle;
  readonly server: Server;
  readonly url: string;
  stop(): Promise<void>;
}

export async function startRustyCrewServiceHost(
  options: RustyCrewServiceHostOptions = {},
): Promise<RustyCrewServiceHost> {
  const env = options.env ?? process.env;
  const config = options.config ?? loadRustyCrewServiceConfig(env);
  assertServiceHostStorageBootReady(
    preflightServiceHostStorageBoot(config, env),
  );
  const app = await createRustyCrewServiceApp({
    ...options,
    env,
    config,
    adapterFactories: {
      ...defaultServiceAdapterFactories(),
      ...options.adapterFactories,
    },
  });
  const server = createServer((request, response) => {
    applyCorsHeaders(request, response);
    if ((request.method ?? "GET").toUpperCase() === "OPTIONS") {
      response.statusCode = 204;
      response.end();
      return;
    }
    void handleHostHttpRequest(request, response, app);
  });

  let backgroundLoopController: ServiceHostBackgroundLoopController | undefined;
  let githubGateConsumerAbort: AbortController | undefined;
  try {
    await listen(server, app.adminPort, app.adminHost);
    backgroundLoopController = startServiceHostBackgroundLoopTimers(
      app.backgroundLoops,
    );
    const reviewUrl = env.RUSTY_CREW_REVIEW_URL?.trim();
    const reviewProjectId = env.RUSTY_CREW_REVIEW_PROJECT_ID?.trim();
    if (reviewUrl && reviewProjectId) {
      githubGateConsumerAbort = new AbortController();
      const consumer = new ReviewGitHubGateEventConsumer({
        baseUrl: new URL(reviewUrl),
        projectId: reviewProjectId,
        bridge: app.bridge,
        bearerToken: env.RUSTY_CREW_REVIEW_BEARER_TOKEN,
      });
      void consumer.run(githubGateConsumerAbort.signal).catch((error) => {
        if (!githubGateConsumerAbort?.signal.aborted) {
          console.warn("Review GitHub gate consumer stopped", error);
        }
      });
    }
  } catch (error) {
    backgroundLoopController?.stop();
    await closeServer(server).catch(() => undefined);
    await app.stop().catch(() => undefined);
    throw error;
  }

  return {
    app,
    config: app.config,
    bridge: app.bridge,
    engine: app.engine,
    server,
    url: app.url,
    stop: async () => {
      backgroundLoopController?.stop();
      githubGateConsumerAbort?.abort();
      const closePromise = closeServer(server);
      await app.stop();
      await closePromise;
    },
  };
}

export type RustyCrewServiceRequestHandler = (
  request: IncomingMessage,
  response: ServerResponse,
) => void;

async function handleHostHttpRequest(
  request: IncomingMessage,
  response: ServerResponse,
  app: RustyCrewServiceApp,
): Promise<void> {
  try {
    const hostResult = await handleHostShellRequest(request, app.config);
    if (hostResult !== undefined) {
      writeHostRouteResult(response, hostResult);
      return;
    }
    app.handle(request, response);
  } catch (error) {
    writeHostRouteResult(
      response,
      hostFailure(500, requestId(request), {
        code: "internal_error",
        reason_code: "service_host_shell_error",
        message: errorMessage(error, "service host shell request failed"),
        retryable: false,
      }),
    );
  }
}

function defaultServiceAdapterFactories(): ServiceAdapterFactories {
  return {
    createDenSuccessorGatewayClient,
    resolveDenConversationChannels,
    createDenMemoryClient,
    createMcpSurfaceManager: (input) => new McpSurfaceManager(input),
    createSimulatedMcpTransportFactory,
    createTelegramAdapterRegistration: (adapterId: AdapterId) =>
      createTelegramAdapterRegistration(adapterId),
    createTelegramConnector: (input) =>
      new TelegramChannelConnector({
        adapterId: input.adapterId,
        bot: createTelegramBotApiHttpClient({
          token: input.botToken,
          baseUrl: input.apiBaseUrl,
          timeoutMs: Math.max(1, input.pollTimeoutSeconds) * 1_000 + 5_000,
        }),
        offsetStore: new FileTelegramUpdateOffsetStore(input.offsetStorePath),
        bindings: input.bindings,
        ttlMs: input.ttlMs,
        pollIntervalMs: input.pollIntervalMs,
        pollTimeoutSeconds: input.pollTimeoutSeconds,
        updateLimit: input.updateLimit,
        now: input.now,
        ingest: async (message) => {
          await input.onInbound(message);
          return { status: "routed" };
        },
      }),
    ingestChannelInboundMessage,
    projectAgentMessageToChannel,
    dispatchChannelMessageProjection,
  };
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}

function listen(server: Server, port: number, host: string): Promise<void> {
  return new Promise((resolveListen, rejectListen) => {
    const onError = (error: Error) => {
      server.off("listening", onListening);
      rejectListen(error);
    };
    const onListening = () => {
      server.off("error", onError);
      resolveListen();
    };
    server.once("error", onError);
    server.once("listening", onListening);
    server.listen(port, host);
  });
}

function closeServer(server: Server): Promise<void> {
  return new Promise((resolveClose, rejectClose) => {
    if (!server.listening) {
      resolveClose();
      return;
    }
    server.close((error) => (error ? rejectClose(error) : resolveClose()));
  });
}

function applyCorsHeaders(
  request: IncomingMessage,
  response: ServerResponse,
): void {
  const origin = headerValue(request, "origin") ?? "*";
  response.setHeader("access-control-allow-origin", origin);
  response.setHeader(
    "access-control-allow-methods",
    "GET,POST,PUT,PATCH,DELETE,OPTIONS",
  );
  response.setHeader(
    "access-control-allow-headers",
    "authorization,content-type,idempotency-key,last-event-id,x-request-id",
  );
  response.setHeader("access-control-expose-headers", "content-type");
  response.setHeader("access-control-max-age", "600");
  response.setHeader("vary", "Origin");
}

function headerValue(
  request: IncomingMessage,
  name: string,
): string | undefined {
  const value = request.headers[name.toLowerCase()];
  return Array.isArray(value) ? value[0] : value;
}
