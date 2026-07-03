import {
  createServer,
  type IncomingMessage,
  type Server,
  type ServerResponse,
} from "node:http";

import {
  createRustyCrewServiceApp,
  type RustyCrewServiceApp,
  type RustyCrewServiceAppOptions,
} from "@rusty-crew/brain-island";
import type { EngineHandle } from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";

export interface RustyCrewServiceHostOptions extends RustyCrewServiceAppOptions {}

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
  const app = await createRustyCrewServiceApp(options);
  const server = createServer((request, response) =>
    app.handle(request, response),
  );

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
