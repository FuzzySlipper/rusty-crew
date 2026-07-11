import { readFileSync } from "node:fs";
import AjvModule, { type ValidateFunction } from "ajv";
import type { ServerNotification } from "../protocol/0.144.1/ts/ServerNotification.js";
import type { ServerRequest } from "../protocol/0.144.1/ts/ServerRequest.js";
import type { JsonRpcId } from "./types.js";

export interface JsonRpcResponseMessage {
  readonly id: JsonRpcId;
  readonly result?: unknown;
  readonly error?: {
    readonly code: number;
    readonly message: string;
    readonly data?: unknown;
  };
}

export type DecodedServerMessage =
  | { readonly type: "response"; readonly response: JsonRpcResponseMessage }
  | { readonly type: "request"; readonly request: ServerRequest }
  | {
      readonly type: "unknown_request";
      readonly id: JsonRpcId;
      readonly method: string;
      readonly params: unknown;
    }
  | { readonly type: "notification"; readonly notification: ServerNotification }
  | {
      readonly type: "unknown_notification";
      readonly method: string;
      readonly params: unknown;
    };

const responseSchemaByMethod: Readonly<Record<string, string>> = Object.freeze({
  initialize: "v1/InitializeResponse.json",
  "thread/list": "v2/ThreadListResponse.json",
  "thread/read": "v2/ThreadReadResponse.json",
  "thread/loaded/list": "v2/ThreadLoadedListResponse.json",
  "thread/start": "v2/ThreadStartResponse.json",
  "thread/resume": "v2/ThreadResumeResponse.json",
  "thread/turns/list": "v2/ThreadTurnsListResponse.json",
  "thread/items/list": "v2/ThreadItemsListResponse.json",
  "turn/start": "v2/TurnStartResponse.json",
  "turn/steer": "v2/TurnSteerResponse.json",
  "turn/interrupt": "v2/TurnInterruptResponse.json",
  "thread/compact/start": "v2/ThreadCompactStartResponse.json",
});

const serverRequestResponseSchemaByMethod: Readonly<Record<string, string>> =
  Object.freeze({
    "item/commandExecution/requestApproval":
      "CommandExecutionRequestApprovalResponse.json",
    "item/fileChange/requestApproval": "FileChangeRequestApprovalResponse.json",
    "item/tool/requestUserInput": "ToolRequestUserInputResponse.json",
    "mcpServer/elicitation/request": "McpServerElicitationRequestResponse.json",
    "item/permissions/requestApproval":
      "PermissionsRequestApprovalResponse.json",
    "item/tool/call": "DynamicToolCallResponse.json",
    "account/chatgptAuthTokens/refresh":
      "ChatgptAuthTokensRefreshResponse.json",
    "attestation/generate": "AttestationGenerateResponse.json",
    "currentTime/read": "CurrentTimeReadResponse.json",
    applyPatchApproval: "ApplyPatchApprovalResponse.json",
    execCommandApproval: "ExecCommandApprovalResponse.json",
  });

export class CodexProtocolError extends Error {
  constructor(
    readonly reasonCode:
      | "malformed_message"
      | "malformed_known_notification"
      | "malformed_known_request"
      | "malformed_response",
    message: string,
    readonly candidate?: unknown,
  ) {
    super(message);
  }
}

export class CodexProtocolCodec {
  readonly #ajv = new AjvConstructor({ allErrors: true, strict: false });
  readonly #validators = new Map<string, ValidateFunction>();
  readonly #knownRequestMethods: ReadonlySet<string>;
  readonly #knownNotificationMethods: ReadonlySet<string>;

  constructor() {
    for (const format of [
      "double",
      "int32",
      "int64",
      "uint",
      "uint16",
      "uint32",
      "uint64",
    ]) {
      this.#ajv.addFormat(format, true);
    }
    this.#knownRequestMethods = this.#methodsFromSchema("ServerRequest.json");
    this.#knownNotificationMethods = this.#methodsFromSchema(
      "ServerNotification.json",
    );
  }

  decode(raw: string): DecodedServerMessage {
    let candidate: unknown;
    try {
      candidate = JSON.parse(raw);
    } catch (error) {
      throw new CodexProtocolError(
        "malformed_message",
        `app-server emitted invalid JSON: ${String(error)}`,
      );
    }
    if (!isRecord(candidate)) {
      throw new CodexProtocolError(
        "malformed_message",
        "app-server message must be an object",
        candidate,
      );
    }
    const hasId = isJsonRpcId(candidate.id);
    const method =
      typeof candidate.method === "string" ? candidate.method : undefined;
    if (method !== undefined && hasId) {
      if (!this.#knownRequestMethods.has(method)) {
        return {
          type: "unknown_request",
          id: candidate.id as JsonRpcId,
          method,
          params: candidate.params,
        };
      }
      this.#assertValid(
        "ServerRequest.json",
        candidate,
        "malformed_known_request",
      );
      return { type: "request", request: candidate as ServerRequest };
    }
    if (method !== undefined && !hasId) {
      if (!this.#knownNotificationMethods.has(method)) {
        return {
          type: "unknown_notification",
          method,
          params: candidate.params,
        };
      }
      this.#assertValid(
        "ServerNotification.json",
        candidate,
        "malformed_known_notification",
      );
      return {
        type: "notification",
        notification: candidate as ServerNotification,
      };
    }
    if (hasId && ("result" in candidate || "error" in candidate)) {
      if ("result" in candidate && "error" in candidate) {
        throw new CodexProtocolError(
          "malformed_response",
          "app-server response cannot contain both result and error",
          candidate,
        );
      }
      if (
        "error" in candidate &&
        candidate.error !== undefined &&
        (!isRecord(candidate.error) ||
          typeof candidate.error.code !== "number" ||
          typeof candidate.error.message !== "string")
      ) {
        throw new CodexProtocolError(
          "malformed_response",
          "app-server response error has an invalid shape",
          candidate,
        );
      }
      return {
        type: "response",
        response: candidate as unknown as JsonRpcResponseMessage,
      };
    }
    throw new CodexProtocolError(
      "malformed_message",
      "app-server message is not a request, response, or notification",
      candidate,
    );
  }

  assertClientResponse(method: string, result: unknown): void {
    const schema = responseSchemaByMethod[method];
    if (schema === undefined) return;
    this.#assertValid(schema, result, "malformed_response");
  }

  assertClientRequest(request: {
    readonly method: string;
    readonly id: JsonRpcId;
    readonly params: unknown;
  }): void {
    this.#assertValid("ClientRequest.json", request, "malformed_message");
  }

  assertServerRequestResolution(method: string, result: unknown): void {
    const schema = serverRequestResponseSchemaByMethod[method];
    if (schema === undefined) {
      throw new CodexProtocolError(
        "malformed_response",
        `no response schema registered for known server request ${method}`,
        result,
      );
    }
    this.#assertValid(schema, result, "malformed_response");
  }

  #methodsFromSchema(path: string): ReadonlySet<string> {
    const schema = this.#readSchema(path) as {
      oneOf?: Array<{ properties?: { method?: { enum?: string[] } } }>;
    };
    return new Set(
      (schema.oneOf ?? []).flatMap(
        (entry) => entry.properties?.method?.enum ?? [],
      ),
    );
  }

  #assertValid(
    path: string,
    value: unknown,
    reasonCode: CodexProtocolError["reasonCode"],
  ): void {
    let validator = this.#validators.get(path);
    if (validator === undefined) {
      const compiled = this.#ajv.compile(this.#readSchema(path));
      this.#validators.set(path, compiled);
      validator = compiled;
    }
    if (!validator(value)) {
      throw new CodexProtocolError(
        reasonCode,
        `${path} validation failed: ${this.#ajv.errorsText(validator.errors)}`,
        value,
      );
    }
  }

  #readSchema(path: string): object {
    return JSON.parse(
      readFileSync(
        new URL(`../protocol/0.144.1/json/${path}`, import.meta.url),
        "utf8",
      ),
    ) as object;
  }
}

const AjvConstructor = AjvModule as unknown as new (options: {
  allErrors: boolean;
  strict: boolean;
}) => {
  addFormat(name: string, format: true): void;
  compile(schema: object): ValidateFunction;
  errorsText(errors: ValidateFunction["errors"]): string;
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isJsonRpcId(value: unknown): value is JsonRpcId {
  return typeof value === "string" || typeof value === "number";
}
