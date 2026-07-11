import type { TSchema } from "typebox";

import {
  bridgeValidationEnabled,
  validateBridgeJsonText,
  type BridgeValidationEnv,
} from "./bridge-validation.js";
import {
  generatedBridgeOutputSchemas,
  type GeneratedBridgeOutputOperation,
} from "./generated/bridge-wire-schemas.js";
import { nativeBridgeBindingSurface } from "./generated/native-binding-surface.js";

const operationByMethod = new Map<string, string>(
  nativeBridgeBindingSurface.methods.flatMap(({ name, operationName }) =>
    operationName === null ? [] : [[name, operationName] as const],
  ),
);

export function withGeneratedBridgeOutputValidation<T extends object>(
  binding: T,
  env: BridgeValidationEnv = process.env,
): T {
  if (!bridgeValidationEnabled(env)) return binding;

  return new Proxy(binding, {
    get(target, property, _receiver) {
      const value = Reflect.get(target, property, target);
      if (typeof property !== "string" || typeof value !== "function") {
        return value;
      }
      const operation = operationByMethod.get(property);
      if (operation === undefined || !hasGeneratedOutputSchema(operation)) {
        return value.bind(target);
      }
      const schema = generatedBridgeOutputSchemas[operation];
      return (...args: unknown[]) => {
        const result = Reflect.apply(value, target, args) as unknown;
        if (result instanceof Promise) {
          return result.then((resolved) => {
            validateJsonOutput(operation, schema, resolved, env);
            return resolved;
          });
        }
        validateJsonOutput(operation, schema, result, env);
        return result;
      };
    },
  });
}

function hasGeneratedOutputSchema(
  operation: string,
): operation is GeneratedBridgeOutputOperation {
  return Object.hasOwn(generatedBridgeOutputSchemas, operation);
}

function validateJsonOutput(
  operation: GeneratedBridgeOutputOperation,
  schema: TSchema,
  value: unknown,
  env: BridgeValidationEnv,
): void {
  if (value === null) {
    validateBridgeJsonText({
      operation,
      direction: "rust_to_ts",
      schema,
      text: "null",
      env,
    });
    return;
  }
  if (typeof value !== "string") {
    throw new TypeError(
      `generated bridge output validation expected JSON text for ${operation}`,
    );
  }
  validateBridgeJsonText({
    operation,
    direction: "rust_to_ts",
    schema,
    text: value,
    env,
  });
}
