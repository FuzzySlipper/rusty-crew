/** JSON records at the crew-services HTTP boundary. */
export interface FabricLease {
  adapter_id: string;
  instance_id: string;
  lease_token: string;
  expires_at: string;
}

export interface FabricBinding {
  address: string;
  bound: boolean;
  adapter_id?: string;
  target_ref?: string;
  capabilities: string[];
  revision: number;
  generation: number;
}

export interface FabricMessage {
  message_id: string;
  sender_address: string;
  recipient_address: string;
  body: string;
  correlation_id?: string;
  reply_to_message_id?: string;
  expires_at: string;
}

export interface FabricDelivery {
  delivery_id: string;
  message_id: string;
  recipient_address: string;
  recipient_generation: number;
  accepted_sequence?: number;
  attempt_count: number;
  state:
    | "queued"
    | "claimed"
    | "dispatching"
    | "delivered"
    | "failed"
    | "expired"
    | "cancelled"
    | "outcome_unknown"
    | string;
  claim_owner_adapter_id?: string;
  claim_owner_instance_id?: string;
  native_attempt_ref?: string;
}

export interface FabricClaim {
  claimed: boolean;
  reason?: string;
  message?: FabricMessage;
  delivery?: FabricDelivery;
  head?: FabricDelivery;
  claim_token?: string;
  replayed: boolean;
}

export interface FabricClient {
  register(input: {
    adapterId: string;
    instanceId: string;
    leaseDuration: string;
    previousLeaseToken?: string;
  }): Promise<FabricLease>;
  renew(input: {
    adapterId: string;
    leaseToken: string;
    leaseDuration: string;
  }): Promise<FabricLease>;
  listBindings(): Promise<{ addresses: FabricBinding[] }>;
  resolveBinding(address: string): Promise<FabricBinding>;
  putBinding(
    address: string,
    body: Record<string, unknown>,
  ): Promise<FabricBinding>;
  unbind(
    address: string,
    body: Record<string, unknown>,
  ): Promise<FabricBinding>;
  submit(body: Record<string, unknown>): Promise<{
    message: FabricMessage;
    delivery: FabricDelivery;
    replayed: boolean;
  }>;
  claim(body: Record<string, unknown>): Promise<FabricClaim>;
  beginDispatch(
    deliveryId: string,
    body: Record<string, unknown>,
  ): Promise<FabricDelivery>;
  release(
    deliveryId: string,
    body: Record<string, unknown>,
  ): Promise<FabricDelivery>;
  acknowledge(
    deliveryId: string,
    body: Record<string, unknown>,
  ): Promise<FabricDelivery>;
  fail(
    deliveryId: string,
    body: Record<string, unknown>,
  ): Promise<FabricDelivery>;
  outcomeUnknown(
    deliveryId: string,
    body: Record<string, unknown>,
  ): Promise<FabricDelivery>;
  listDeliveries(): Promise<{ deliveries: FabricDelivery[] }>;
  getDelivery(deliveryId: string): Promise<FabricDelivery>;
}

export class CrewServicesHttpError extends Error {
  constructor(
    readonly code: string,
    message = `crew-services ${code}`,
  ) {
    super(message);
  }
}

export const directBrainCapabilities = [
  "deliver_when_idle",
  "durable_next_turn",
  "wake_inactive",
] as const;

export function operationId(deliveryId: string, action: string): string {
  return `rusty-crew-fabric:${deliveryId}:${action}`;
}

export function nativeDeliveryId(deliveryId: string): string {
  return `fabric-delivery:${deliveryId}`;
}

export function nativeMessageId(messageId: string): string {
  return `fabric-message:${messageId}`;
}

export function nativeAttemptRef(deliveryId: string): string {
  return `rusty-crew-fabric:${deliveryId}:native`;
}
