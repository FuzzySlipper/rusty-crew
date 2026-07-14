import { readFileSync } from "node:fs";
import AjvModule, { type ValidateFunction } from "ajv";
import type { ServerNotification } from "../protocol/0.144.1/ts/ServerNotification.js";
import type { ServerRequest } from "../protocol/0.144.1/ts/ServerRequest.js";
import {
  allowAdditiveObjectFields,
  CODEX_CONSUMED_INBOUND_SCHEMAS,
  CODEX_CONSUMED_RESPONSE_SCHEMAS,
  CODEX_CONSUMED_SERVER_REQUEST_RESPONSE_SCHEMAS,
} from "./consumed-contract.js";
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
    this.#knownRequestMethods = new Set(
      Object.keys(CODEX_CONSUMED_SERVER_REQUEST_RESPONSE_SCHEMAS),
    );
    this.#knownNotificationMethods = this.#methodsFromSchema(
      CODEX_CONSUMED_INBOUND_SCHEMAS.serverNotification,
    );
  }

  assertConsumedContractReady(): void {
    for (const path of Object.values(CODEX_CONSUMED_INBOUND_SCHEMAS)) {
      this.#validator(path, true);
    }
    for (const path of Object.values(CODEX_CONSUMED_RESPONSE_SCHEMAS)) {
      this.#validator(path, true);
    }
    for (const path of Object.values(
      CODEX_CONSUMED_SERVER_REQUEST_RESPONSE_SCHEMAS,
    )) {
      this.#validator(path, false);
    }
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
        CODEX_CONSUMED_INBOUND_SCHEMAS.serverRequest,
        candidate,
        "malformed_known_request",
        true,
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
        CODEX_CONSUMED_INBOUND_SCHEMAS.serverNotification,
        candidate,
        "malformed_known_notification",
        true,
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
    const schema = CODEX_CONSUMED_RESPONSE_SCHEMAS[method];
    if (schema === undefined) {
      throw new CodexProtocolError(
        "malformed_response",
        `no consumed response contract registered for ${method}`,
        result,
      );
    }
    this.#assertValid(schema, result, "malformed_response", true);
  }

  assertClientRequest(request: {
    readonly method: string;
    readonly id: JsonRpcId;
    readonly params: unknown;
  }): void {
    this.#assertValid("ClientRequest.json", request, "malformed_message");
  }

  assertServerRequestResolution(method: string, result: unknown): void {
    const schema = CODEX_CONSUMED_SERVER_REQUEST_RESPONSE_SCHEMAS[method];
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
    allowAdditiveFields = false,
  ): void {
    const validatorKey = `${path}:${allowAdditiveFields ? "additive" : "exact"}`;
    const validator = this.#validator(path, allowAdditiveFields);
    if (!validator(value)) {
      throw new CodexProtocolError(
        reasonCode,
        `${path} validation failed: ${this.#ajv.errorsText(validator.errors)}`,
        value,
      );
    }
  }

  #validator(path: string, allowAdditiveFields: boolean): ValidateFunction {
    const validatorKey = `${path}:${allowAdditiveFields ? "additive" : "exact"}`;
    const existing = this.#validators.get(validatorKey);
    if (existing !== undefined) return existing;
    const schema = this.#readSchema(path);
    const compiled = this.#ajv.compile(
      allowAdditiveFields ? allowAdditiveObjectFields(schema) : schema,
    );
    this.#validators.set(validatorKey, compiled);
    return compiled;
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
