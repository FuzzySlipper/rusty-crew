import {
  createServer,
  type IncomingMessage,
  type Server,
  type ServerResponse,
} from "node:http";

import {
  createRustyCrewServiceApp,
  type ServiceAdapterFactories,
  type RustyCrewServiceApp,
  type RustyCrewServiceAppOptions,
} from "@rusty-crew/brain-island";
import {
  createDenMemoryClient,
  createDenSuccessorGatewayClient,
  dispatchChannelMessageProjection,
  ingestChannelInboundMessage,
  projectAgentMessageToChannel,
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
  const app = await createRustyCrewServiceApp({
    ...options,
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
    app.handle(request, response);
  });

  try {
    await listen(server, app.adminPort, app.adminHost);
  } catch (error) {
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

function defaultServiceAdapterFactories(): ServiceAdapterFactories {
  return {
    createDenSuccessorGatewayClient,
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
