import {
  CrewServicesHttpError,
  type FabricBinding,
  type FabricClaim,
  type FabricClient,
  type FabricDelivery,
  type FabricLease,
  type FabricMessage,
} from "./types.js";

/** Small typed client for the local, runtime-neutral crew-services boundary. */
export class CrewServicesClient implements FabricClient {
  constructor(
    private readonly baseUrl: string,
    private readonly request: typeof fetch = fetch,
  ) {}

  private async call<T>(
    path: string,
    method = "GET",
    body?: unknown,
  ): Promise<T> {
    const response = await this.request(new URL(path, this.baseUrl), {
      method,
      ...(body === undefined
        ? {}
        : {
            headers: { "content-type": "application/json" },
            body: JSON.stringify(body),
          }),
    });
    const value: unknown = await response.json().catch(() => ({}));
    if (!response.ok) {
      const code =
        typeof value === "object" &&
        value !== null &&
        "code" in value &&
        typeof value.code === "string"
          ? value.code
          : `http_${response.status}`;
      throw new CrewServicesHttpError(code);
    }
    return value as T;
  }

  register(input: {
    adapterId: string;
    instanceId: string;
    leaseDuration: string;
    previousLeaseToken?: string;
  }): Promise<FabricLease> {
    return this.call("/v1/adapters/register", "POST", {
      adapter_id: input.adapterId,
      instance_id: input.instanceId,
      lease_duration: input.leaseDuration,
      ...(input.previousLeaseToken === undefined
        ? {}
        : { previous_lease_token: input.previousLeaseToken }),
    });
  }
  renew(input: {
    adapterId: string;
    leaseToken: string;
    leaseDuration: string;
  }): Promise<FabricLease> {
    return this.call("/v1/adapters/renew", "POST", {
      adapter_id: input.adapterId,
      lease_token: input.leaseToken,
      lease_duration: input.leaseDuration,
    });
  }
  listBindings(): Promise<{ addresses: FabricBinding[] }> {
    return this.call("/v1/addresses");
  }
  resolveBinding(address: string): Promise<FabricBinding> {
    return this.call(`/v1/addresses/${encodeURIComponent(address)}`);
  }
  putBinding(
    address: string,
    body: Record<string, unknown>,
  ): Promise<FabricBinding> {
    return this.call(
      `/v1/addresses/${encodeURIComponent(address)}/binding`,
      "PUT",
      body,
    );
  }
  unbind(
    address: string,
    body: Record<string, unknown>,
  ): Promise<FabricBinding> {
    return this.call(
      `/v1/addresses/${encodeURIComponent(address)}/binding`,
      "DELETE",
      body,
    );
  }
  submit(body: Record<string, unknown>): Promise<{
    message: FabricMessage;
    delivery: FabricDelivery;
    replayed: boolean;
  }> {
    return this.call("/v1/messages", "POST", body);
  }
  claim(body: Record<string, unknown>): Promise<FabricClaim> {
    return this.call("/v1/deliveries/claim", "POST", body);
  }
  beginDispatch(
    deliveryId: string,
    body: Record<string, unknown>,
  ): Promise<FabricDelivery> {
    return this.call(
      `/v1/deliveries/${encodeURIComponent(deliveryId)}/begin-dispatch`,
      "POST",
      body,
    );
  }
  release(
    deliveryId: string,
    body: Record<string, unknown>,
  ): Promise<FabricDelivery> {
    return this.call(
      `/v1/deliveries/${encodeURIComponent(deliveryId)}/release`,
      "POST",
      body,
    );
  }
  acknowledge(
    deliveryId: string,
    body: Record<string, unknown>,
  ): Promise<FabricDelivery> {
    return this.call(
      `/v1/deliveries/${encodeURIComponent(deliveryId)}/acknowledge`,
      "POST",
      body,
    );
  }
  fail(
    deliveryId: string,
    body: Record<string, unknown>,
  ): Promise<FabricDelivery> {
    return this.call(
      `/v1/deliveries/${encodeURIComponent(deliveryId)}/fail`,
      "POST",
      body,
    );
  }
  outcomeUnknown(
    deliveryId: string,
    body: Record<string, unknown>,
  ): Promise<FabricDelivery> {
    return this.call(
      `/v1/deliveries/${encodeURIComponent(deliveryId)}/outcome-unknown`,
      "POST",
      body,
    );
  }
  listDeliveries(): Promise<{ deliveries: FabricDelivery[] }> {
    return this.call("/v1/deliveries");
  }
  getDelivery(deliveryId: string): Promise<FabricDelivery> {
    return this.call(`/v1/deliveries/${encodeURIComponent(deliveryId)}`);
  }
}
