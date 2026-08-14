import type {
  AgentCoordinationCaller,
  TelegramOperatorConsultCategory,
} from "@rusty-crew/contracts";
import { Type, type Static } from "typebox";
import type { BrainTool, BrainToolResult } from "./brain-tool.js";
import type { BrainToolResolver } from "./tool-session-selection.js";

const requestTelegramConsultParameters = Type.Object(
  {
    message: Type.String({ minLength: 1, maxLength: 4096 }),
    category: Type.Optional(
      Type.Union([
        Type.Literal("network_trouble"),
        Type.Literal("ambiguous_request"),
        Type.Literal("unfamiliar_machine_state"),
        Type.Literal("other"),
      ]),
    ),
  },
  { additionalProperties: false },
);

export type RequestTelegramConsultParameters = Static<
  typeof requestTelegramConsultParameters
>;

export interface TelegramConsultToolReceipt {
  ok: boolean;
  consultId?: string;
  status?: "pending" | "sent" | "failed";
  duplicate?: boolean;
  bindingId?: string;
  externalMessageIds?: string[];
  reasonCode?: string;
  summary: string;
}

export interface TelegramConsultToolRuntime {
  request(
    input: RequestTelegramConsultParameters & {
      caller: AgentCoordinationCaller;
      originatingWakeKind?: string;
    },
  ): Promise<TelegramConsultToolReceipt>;
}

export function createTelegramConsultToolResolver(
  runtime?: TelegramConsultToolRuntime,
): BrainToolResolver {
  return () => [requestTelegramConsultTool(runtime)];
}

export function requestTelegramConsultTool(
  runtime?: TelegramConsultToolRuntime,
): BrainTool<
  typeof requestTelegramConsultParameters,
  TelegramConsultToolReceipt
> {
  return {
    name: "request_telegram_consult",
    label: "Request Telegram consult",
    description:
      "Send one concise technical consult request to the remote operator through this exact session's active Telegram diplomat binding. Include what you observed and a specific question. Do not use repeatedly for the same issue. The current turn continues on its original surface.",
    parameters: requestTelegramConsultParameters,
    executeWithContext: async (params, context) => {
      if (runtime === undefined) {
        return result({
          ok: false,
          reasonCode: "telegram_consult_runtime_unavailable",
          summary: "The Telegram consult runtime is unavailable.",
        });
      }
      try {
        const pending = context.wake.state.pendingMessages[0] as
          | { inputKind?: string }
          | undefined;
        const receipt = await runtime.request({
          ...params,
          caller: {
            type: "direct_brain",
            sessionId: context.sessionId,
            wakeId: context.wakeId,
            toolCallId: context.callId,
          },
          ...(pending?.inputKind === undefined
            ? {}
            : { originatingWakeKind: pending.inputKind }),
        });
        return result(receipt);
      } catch (error) {
        return result({
          ok: false,
          reasonCode: "telegram_consult_request_rejected",
          summary:
            error instanceof Error
              ? `Telegram consult was not sent: ${error.message}`
              : "Telegram consult was not sent.",
        });
      }
    },
    execute: async () =>
      result({
        ok: false,
        reasonCode: "tool_context_required",
        summary: "request_telegram_consult requires trusted wake context.",
      }),
  };
}

function result(
  receipt: TelegramConsultToolReceipt,
): BrainToolResult<TelegramConsultToolReceipt> {
  return {
    content: [{ type: "text", text: receipt.summary }],
    details: receipt,
  };
}

export function telegramConsultCategory(
  category: RequestTelegramConsultParameters["category"],
): TelegramOperatorConsultCategory | undefined {
  return category;
}
