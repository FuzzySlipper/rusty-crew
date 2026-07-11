import WebSocket from "ws";

export interface CodexTransportHandlers {
  readonly onMessage: (message: string) => void;
  readonly onClose: (reason: string) => void;
  readonly onError: (error: Error) => void;
}

export interface CodexJsonRpcTransport {
  setHandlers(handlers: CodexTransportHandlers): void;
  open(): Promise<void>;
  send(message: string): Promise<void>;
  close(): Promise<void>;
}

export class UnixWebSocketTransport implements CodexJsonRpcTransport {
  readonly #socketPath: string;
  readonly #handshakeTimeoutMs: number;
  #handlers?: CodexTransportHandlers;
  #socket: WebSocket | undefined;
  #sendChain: Promise<void> = Promise.resolve();

  constructor(socketPath: string, handshakeTimeoutMs = 30_000) {
    if (!socketPath.startsWith("/")) {
      throw new Error("Codex app-server Unix socket path must be absolute");
    }
    this.#socketPath = socketPath;
    this.#handshakeTimeoutMs = handshakeTimeoutMs;
  }

  setHandlers(handlers: CodexTransportHandlers): void {
    this.#handlers = handlers;
  }

  async open(): Promise<void> {
    if (this.#handlers === undefined) {
      throw new Error("Codex transport handlers must be installed before open");
    }
    if (this.#socket !== undefined) {
      throw new Error("Codex transport is already open");
    }
    const socket = new WebSocket(`ws+unix://${this.#socketPath}:/`, {
      perMessageDeflate: false,
      handshakeTimeout: this.#handshakeTimeoutMs,
    });
    this.#socket = socket;
    socket.on("message", (data) => this.#handlers?.onMessage(data.toString()));
    socket.on("close", (code, reason) =>
      this.#handlers?.onClose(
        `websocket closed code=${code} reason=${reason.toString()}`,
      ),
    );
    socket.on("error", (error) => this.#handlers?.onError(error));
    await new Promise<void>((resolve, reject) => {
      const onOpen = (): void => {
        socket.off("error", onInitialError);
        resolve();
      };
      const onInitialError = (error: Error): void => {
        socket.off("open", onOpen);
        reject(error);
      };
      socket.once("open", onOpen);
      socket.once("error", onInitialError);
    });
  }

  send(message: string): Promise<void> {
    const operation = this.#sendChain.then(
      () =>
        new Promise<void>((resolve, reject) => {
          const socket = this.#socket;
          if (socket?.readyState !== WebSocket.OPEN) {
            reject(new Error("Codex app-server WebSocket is not open"));
            return;
          }
          socket.send(message, (error) => {
            // ws currently reports successful UDS writes with null at runtime
            // despite its callback declaration using an optional Error.
            if (error == null) resolve();
            else reject(error);
          });
        }),
    );
    this.#sendChain = operation.catch(() => undefined);
    return operation;
  }

  async close(): Promise<void> {
    const socket = this.#socket;
    this.#socket = undefined;
    if (socket === undefined || socket.readyState === WebSocket.CLOSED) return;
    await new Promise<void>((resolve) => {
      const timer = setTimeout(resolve, 1_000);
      socket.once("close", () => {
        clearTimeout(timer);
        resolve();
      });
      socket.close();
    });
  }
}
