import type { TSchema } from "typebox";
import { Value } from "typebox/value";

export type BridgeValidationDirection = "ts_to_rust" | "rust_to_ts";

export interface BridgeValidationIssue {
  path?: string;
  schemaPath?: string;
  message: string;
}

export interface BridgeValidationEnv {
  RUSTY_CREW_BRIDGE_VALIDATE?: string;
}

export class BridgeValidationError extends Error {
  readonly operation: string;
  readonly direction: BridgeValidationDirection;
  readonly issues: BridgeValidationIssue[];

  constructor(input: {
    operation: string;
    direction: BridgeValidationDirection;
    issues: BridgeValidationIssue[];
  }) {
    const details = input.issues
      .map((issue) => {
        const location = issue.path ?? issue.schemaPath ?? "$";
        return `${location}: ${issue.message}`;
      })
      .join("; ");
    super(
      `bridge validation failed for ${input.operation} (${input.direction}): ${details}`,
    );
    this.name = "BridgeValidationError";
    this.operation = input.operation;
    this.direction = input.direction;
    this.issues = input.issues;
  }
}

export function bridgeValidationEnabled(
  env: BridgeValidationEnv = process.env,
): boolean {
  return env.RUSTY_CREW_BRIDGE_VALIDATE === "1";
}

export function validateBridgeValue<T>(input: {
  operation: string;
  direction: BridgeValidationDirection;
  schema: TSchema;
  value: unknown;
  env?: BridgeValidationEnv;
}): T {
  if (!bridgeValidationEnabled(input.env)) {
    return input.value as T;
  }

  if (Value.Check(input.schema, input.value)) {
    return input.value as T;
  }

  const issues = [...Value.Errors(input.schema, input.value)]
    .slice(0, 8)
    .map((error): BridgeValidationIssue => {
      const errorRecord = error as unknown as Record<string, unknown>;
      const path =
        typeof errorRecord.path === "string" ? errorRecord.path : undefined;
      const schemaPath =
        typeof errorRecord.schemaPath === "string"
          ? errorRecord.schemaPath
          : undefined;
      return {
        ...(path === undefined ? {} : { path }),
        ...(schemaPath === undefined ? {} : { schemaPath }),
        message: error.message,
      };
    });

  throw new BridgeValidationError({
    operation: input.operation,
    direction: input.direction,
    issues,
  });
}

export function validateBridgeJsonText<T>(input: {
  operation: string;
  direction: BridgeValidationDirection;
  schema: TSchema;
  text: string;
  env?: BridgeValidationEnv;
}): T | undefined {
  if (!bridgeValidationEnabled(input.env)) {
    return undefined;
  }

  try {
    return validateBridgeValue<T>({
      operation: input.operation,
      direction: input.direction,
      schema: input.schema,
      value: JSON.parse(input.text),
      env: input.env,
    });
  } catch (error) {
    if (error instanceof SyntaxError) {
      throw new BridgeValidationError({
        operation: input.operation,
        direction: input.direction,
        issues: [{ message: `invalid JSON: ${error.message}` }],
      });
    }
    throw error;
  }
}

export function assertBridgeValue(input: {
  operation: string;
  direction: BridgeValidationDirection;
  schema: TSchema;
  value: unknown;
  env?: BridgeValidationEnv;
}): void {
  validateBridgeValue(input);
}
