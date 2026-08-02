import type {
  AgentCorrelatedRound,
  AgentDirectoryEntry,
  AgentMessageCommand,
  AgentMessageDeliveryCompletion,
  AgentMessageDeliveryReceipt,
  AgentMessageInboxItem,
  AgentMessageInboxQuery,
  AgentMessageReplyCommand,
  AgentMessageTrafficItem,
  AgentRouteDelete,
  AgentRouteRecord,
  AgentRouteResolution,
  AgentRouteWrite,
  AgentRoundCommand,
  AgentRoundStartReceipt,
} from "@rusty-crew/contracts";

export interface NativeAgentCoordinationBridgeMethods {
  deliverAgentMessage(
    command: AgentMessageCommand,
  ): Promise<AgentMessageDeliveryReceipt>;
  replyAgentMessage(
    command: AgentMessageReplyCommand,
  ): Promise<AgentMessageDeliveryReceipt>;
  listAgentMessageInbox(
    query: AgentMessageInboxQuery,
  ): Promise<AgentMessageInboxItem[]>;
  listAgentMessageTraffic(
    query: AgentMessageInboxQuery,
  ): Promise<AgentMessageTrafficItem[]>;
  listAgentDirectory(): Promise<AgentDirectoryEntry[]>;
  listAgentRouteResolutions(): Promise<AgentRouteResolution[]>;
  getAgentRouteResolution(
    routeKey: string,
  ): Promise<AgentRouteResolution | undefined>;
  resolveAgentAddress(address: string): Promise<AgentRouteResolution>;
  putAgentRoute(write: AgentRouteWrite): Promise<AgentRouteRecord>;
  deleteAgentRoute(deleteRequest: AgentRouteDelete): Promise<AgentRouteRecord>;
  beginAgentRound(command: AgentRoundCommand): Promise<AgentRoundStartReceipt>;
  getAgentRound(roundId: string): Promise<AgentCorrelatedRound | undefined>;
  getAgentMessageDelivery(
    deliveryId: string,
  ): Promise<AgentMessageDeliveryReceipt | undefined>;
  completeAgentMessageDelivery(
    completion: AgentMessageDeliveryCompletion,
  ): Promise<AgentMessageDeliveryReceipt>;
}
