import assert from "node:assert/strict";
import test from "node:test";
import type {
  AgentDirectoryEntry,
  AgentMessageDeliveryReceipt,
  AgentRouteResolution,
} from "@rusty-crew/contracts";
import { CrewServicesDirectBrainAdapter } from "./adapter.js";
import {
  nativeAttemptRef,
  nativeDeliveryId,
  type FabricBinding,
  type FabricClaim,
  type FabricClient,
  type FabricDelivery,
  type FabricLease,
} from "./types.js";

const routeKey = "@brain-beta";
const route = (revision = 1): AgentRouteResolution => ({
  address: routeKey,
  routable: true,
  route: {
    createdAt: "2026-01-01T00:00:00.000Z",
    updatedAt: "2026-01-01T00:00:00.000Z",
    enabled: true,
    label: "Beta",
    revision,
    routeKey,
    target: {
      type: "direct_brain",
      agentId: "beta-agent",
      sessionId: "beta-session",
    },
  },
  resolvedTarget: {
    agentId: "beta-agent",
    sessionId: "beta-session",
    displayLabel: "Beta",
    profileId: "default",
    runtimeKind: "direct_brain",
  },
});

const entry = (): AgentDirectoryEntry => ({
  agentId: "beta-agent",
  displayLabel: "Beta",
  profileId: "default",
  runtimeKind: "direct_brain",
  routable: true,
  sessionId: "beta-session",
  sessionKind: "full",
  sessionStatus: "idle",
});

const message = (id = "m-1") => ({
  message_id: id,
  sender_address: "alpha",
  recipient_address: "beta",
  body: "please review",
  expires_at: "2030-01-01T00:00:00.000Z",
});
const delivery = (id = "d-1"): FabricDelivery => ({
  delivery_id: id,
  message_id: `m-${id}`,
  recipient_address: "beta",
  recipient_generation: 1,
  attempt_count: 0,
  state: "claimed",
  claim_owner_adapter_id: "rusty-crew-fabric",
});

class FakeFabric implements FabricClient {
  bindings: FabricBinding[] = [];
  queue: FabricClaim[] = [];
  dispatching: FabricDelivery[] = [];
  calls: string[] = [];
  claimOperations: string[] = [];
  claimAvailabilities: string[] = [];
  releaseOperations: string[] = [];
  registerCount = 0;
  onClaim: (() => void) | undefined;
  beginGate: Promise<void> | undefined;
  claimedRows: FabricDelivery[] = [];
  replayedClaims = new Map<string, FabricClaim>();
  deliveryClaims = new Map<string, FabricClaim>();
  throwAfterClaim = false;
  throwAfterBegin = false;
  getDeliveryFailures = 0;
  async register(): Promise<FabricLease> {
    this.registerCount += 1;
    return {
      adapter_id: "rusty-crew-fabric",
      instance_id: "test",
      lease_token: `lease-${this.registerCount}`,
      expires_at: "2030-01-01T00:00:00.000Z",
    };
  }
  async renew(): Promise<FabricLease> {
    return this.register();
  }
  async listBindings(): Promise<{ addresses: FabricBinding[] }> {
    return { addresses: this.bindings };
  }
  async resolveBinding(address: string): Promise<FabricBinding> {
    const found = this.bindings.find(
      (candidate) => candidate.address === address,
    );
    if (found === undefined) throw new Error("missing binding");
    return found;
  }
  async putBinding(
    address: string,
    body: Record<string, unknown>,
  ): Promise<FabricBinding> {
    const prior = this.bindings.find(
      (candidate) => candidate.address === address,
    );
    const next: FabricBinding = {
      address,
      bound: true,
      adapter_id: String(body.adapter_id),
      target_ref: String(body.target_ref),
      capabilities: [...(body.capabilities as string[])],
      revision: (prior?.revision ?? 0) + 1,
      generation: (prior?.generation ?? 0) + 1,
    };
    this.bindings = [
      ...this.bindings.filter((candidate) => candidate.address !== address),
      next,
    ];
    return next;
  }
  async unbind(): Promise<FabricBinding> {
    throw new Error("not used");
  }
  async submit(): Promise<{
    message: ReturnType<typeof message>;
    delivery: FabricDelivery;
    replayed: boolean;
  }> {
    return { message: message(), delivery: delivery(), replayed: false };
  }
  async claim(body: Record<string, unknown>): Promise<FabricClaim> {
    this.onClaim?.();
    this.onClaim = undefined;
    const operation = String(body.operation_id);
    this.claimOperations.push(operation);
    this.claimAvailabilities.push(String(body.availability));
    const replayed = this.replayedClaims.get(operation);
    if (replayed !== undefined) return { ...replayed, replayed: true };
    const next = this.queue.shift();
    if (next === undefined) return { claimed: false, replayed: false };
    const claimed: FabricClaim =
      next.delivery === undefined
        ? next
        : {
            ...next,
            delivery: {
              ...next.delivery,
              state: "claimed",
              attempt_count: next.delivery.attempt_count + 1,
              claim_owner_adapter_id: "rusty-crew-fabric",
              claim_owner_instance_id: "test",
              dispatch_action:
                body.availability === "busy"
                  ? "register_next_turn"
                  : body.availability === "idle"
                    ? "deliver_at_idle"
                    : "wake_then_deliver",
            },
          };
    this.replayedClaims.set(operation, claimed);
    if (claimed.delivery !== undefined) {
      this.deliveryClaims.set(claimed.delivery.delivery_id, claimed);
    }
    if (this.throwAfterClaim && claimed.delivery !== undefined) {
      this.throwAfterClaim = false;
      this.claimedRows.push({
        ...claimed.delivery,
      });
      throw new Error("claim response lost");
    }
    return claimed;
  }
  async beginDispatch(id: string): Promise<FabricDelivery> {
    this.calls.push(`begin:${id}`);
    await this.beginGate;
    const value = {
      ...(this.deliveryClaims.get(id)?.delivery ?? delivery(id)),
      state: "dispatching" as const,
      native_attempt_ref: nativeAttemptRef(id),
    };
    this.claimedRows = this.claimedRows.filter(
      (candidate) => candidate.delivery_id !== id,
    );
    this.dispatching.push(value);
    if (this.throwAfterBegin) {
      this.throwAfterBegin = false;
      throw new Error("begin response lost");
    }
    return value;
  }
  async release(
    id: string,
    body: Record<string, unknown>,
  ): Promise<FabricDelivery> {
    this.calls.push(`release:${id}`);
    this.releaseOperations.push(String(body.operation_id));
    const claimed = this.deliveryClaims.get(id);
    if (claimed?.delivery !== undefined) {
      this.queue.unshift({
        ...claimed,
        delivery: { ...claimed.delivery, state: "queued", dispatch_action: "" },
      });
    }
    return { ...(claimed?.delivery ?? delivery(id)), state: "queued" };
  }
  async acknowledge(id: string): Promise<FabricDelivery> {
    this.calls.push(`ack:${id}`);
    this.dispatching = this.dispatching.filter(
      (candidate) => candidate.delivery_id !== id,
    );
    return { ...delivery(id), state: "delivered" };
  }
  async fail(id: string): Promise<FabricDelivery> {
    this.calls.push(`fail:${id}`);
    this.dispatching = this.dispatching.filter(
      (candidate) => candidate.delivery_id !== id,
    );
    return { ...delivery(id), state: "failed" };
  }
  async outcomeUnknown(id: string): Promise<FabricDelivery> {
    this.calls.push(`unknown:${id}`);
    this.dispatching = this.dispatching.filter(
      (candidate) => candidate.delivery_id !== id,
    );
    return { ...delivery(id), state: "outcome_unknown" };
  }
  async listDeliveries(): Promise<{ deliveries: FabricDelivery[] }> {
    return {
      deliveries: [
        ...this.queue.flatMap((claim, index) =>
          claim.delivery === undefined
            ? []
            : [
                {
                  ...claim.delivery,
                  state: "queued" as const,
                  accepted_sequence: index + 1,
                },
              ],
        ),
        ...this.claimedRows,
        ...this.dispatching,
      ],
    };
  }
  async getDelivery(id: string): Promise<FabricDelivery> {
    if (this.getDeliveryFailures > 0) {
      this.getDeliveryFailures -= 1;
      throw new Error("delivery readback unavailable");
    }
    const found = (await this.listDeliveries()).deliveries.find(
      (candidate) => candidate.delivery_id === id,
    );
    if (found === undefined) throw new Error("missing delivery");
    return found;
  }
}

class FakeBridge {
  resolution = route();
  directory = [entry()];
  receipts = new Map<string, AgentMessageDeliveryReceipt>();
  delivered: string[] = [];
  throwDelivery = false;
  rejection = false;
  readbackFailures = 0;
  async listAgentDirectory(): Promise<AgentDirectoryEntry[]> {
    return this.directory;
  }
  async getAgentRouteResolution(): Promise<AgentRouteResolution> {
    return this.resolution;
  }
  async deliverAgentMessage(command: {
    deliveryId: string;
  }): Promise<AgentMessageDeliveryReceipt> {
    this.delivered.push(command.deliveryId);
    if (this.throwDelivery) throw new Error("native boundary disconnected");
    const status = this.rejection ? "rejected" : "accepted";
    const receipt = {
      status,
      revision: 1,
      request: {},
    } as AgentMessageDeliveryReceipt;
    this.receipts.set(command.deliveryId, receipt);
    return receipt;
  }
  async getAgentMessageDelivery(
    id: string,
  ): Promise<AgentMessageDeliveryReceipt | undefined> {
    if (this.readbackFailures > 0) {
      this.readbackFailures -= 1;
      throw new Error("native readback unavailable");
    }
    return this.receipts.get(id);
  }
}

function adapter(
  fabric = new FakeFabric(),
  bridge = new FakeBridge(),
  options: Partial<{ renewMs: number }> = {},
) {
  return [
    new CrewServicesDirectBrainAdapter(fabric, bridge, {
      adapterId: "rusty-crew-fabric",
      instanceId: "test",
      bindings: [{ alias: "beta", routeKey, routeRevision: 1 }],
      pollMs: 60_000,
      ...options,
    }),
    fabric,
    bridge,
  ] as const;
}

test("delivers one claimed envelope through one deterministic native receipt before acknowledgement", async () => {
  const [subject, fabric, bridge] = adapter();
  fabric.queue.push({
    claimed: true,
    replayed: false,
    claim_token: "claim",
    delivery: delivery(),
    message: message(),
  });
  await subject.start();
  await subject.tick();
  assert.deepEqual(bridge.delivered, [nativeDeliveryId("d-1")]);
  assert.deepEqual(fabric.calls, ["begin:d-1", "ack:d-1"]);
  await subject.stop();
});

test("a replayed claim produces one native delivery, not a second claim-side effect", async () => {
  const [subject, fabric, bridge] = adapter();
  const replay: FabricClaim = {
    claimed: true,
    replayed: true,
    claim_token: "claim",
    delivery: delivery(),
    message: message(),
  };
  fabric.queue.push(replay);
  await subject.start();
  await subject.tick();
  await subject.tick();
  assert.equal(bridge.delivered.length, 1);
  await subject.stop();
});

test("a lost claim response replays the delivery-keyed operation after restart without skipping FIFO", async () => {
  const fabric = new FakeFabric();
  const bridge = new FakeBridge();
  bridge.directory = [{ ...entry(), sessionStatus: "active" }];
  fabric.throwAfterClaim = true;
  fabric.queue.push(
    {
      claimed: true,
      replayed: false,
      claim_token: "first-claim",
      delivery: delivery("d-first"),
      message: message("m-first"),
    },
    {
      claimed: true,
      replayed: false,
      claim_token: "second-claim",
      delivery: delivery("d-second"),
      message: message("m-second"),
    },
  );
  const [first] = adapter(fabric, bridge);
  await first.start();
  await assert.rejects(first.tick(), /claim response lost/);
  await first.stop();
  bridge.directory = [entry()];
  const [restarted] = adapter(fabric, bridge);
  await restarted.start();
  await restarted.tick();
  await restarted.tick();
  assert.deepEqual(fabric.claimOperations.slice(0, 2), [
    fabric.claimOperations[0],
    fabric.claimOperations[0],
  ]);
  assert.deepEqual(bridge.delivered, [
    nativeDeliveryId("d-first"),
    nativeDeliveryId("d-second"),
  ]);
  assert.deepEqual(fabric.claimAvailabilities.slice(0, 2), ["busy", "busy"]);
  await restarted.stop();
});

test("route revision drift after claim releases before native dispatch", async () => {
  const [subject, fabric, bridge] = adapter();
  fabric.queue.push({
    claimed: true,
    replayed: false,
    claim_token: "claim",
    delivery: delivery(),
    message: message(),
  });
  fabric.onClaim = () => {
    bridge.resolution = route(2);
  };
  await subject.start();
  await subject.tick();
  assert.deepEqual(fabric.calls, ["release:d-1"]);
  assert.deepEqual(bridge.delivered, []);
  await subject.stop();
});

test("each route-drift release is idempotent for its own attempt before attempt three succeeds", async () => {
  const [subject, fabric, bridge] = adapter();
  fabric.queue.push({
    claimed: true,
    replayed: false,
    claim_token: "first-claim",
    delivery: delivery(),
    message: message(),
  });
  fabric.onClaim = () => {
    bridge.resolution = route(2);
  };
  await subject.start();
  await subject.tick();
  assert.ok(fabric.calls.includes("release:d-1"));
  bridge.resolution = route(1);
  fabric.onClaim = () => {
    bridge.resolution = route(2);
  };
  await subject.tick();
  bridge.resolution = route(1);
  await subject.tick();
  assert.equal(fabric.claimOperations.length, 3);
  assert.equal(fabric.releaseOperations.length, 2);
  assert.notEqual(fabric.releaseOperations[0], fabric.releaseOperations[1]);
  assert.ok(fabric.releaseOperations[0]?.includes("d-1:attempt:1"));
  assert.ok(fabric.releaseOperations[1]?.includes("d-1:attempt:2"));
  assert.ok(fabric.claimOperations[2]?.includes("d-1:attempt:3"));
  assert.ok(fabric.calls.includes("begin:d-1"));
  assert.deepEqual(bridge.delivered, [nativeDeliveryId("d-1")]);
  await subject.stop();
});

test("a stale-generation queued row cannot select the current generation claim operation", async () => {
  const [subject, fabric] = adapter();
  fabric.queue.push(
    {
      claimed: true,
      replayed: false,
      claim_token: "stale-claim",
      delivery: { ...delivery("d-stale"), recipient_generation: 99 },
      message: message("m-stale"),
    },
    {
      claimed: true,
      replayed: false,
      claim_token: "current-claim",
      delivery: delivery("d-current"),
      message: message("m-current"),
    },
  );
  await subject.start();
  await subject.tick();
  assert.ok(fabric.claimOperations[0]?.includes("d-current:attempt:1"));
  await subject.stop();
});

test("native rejection is terminal fabric failure and ambiguous native delivery is read back", async () => {
  const [subject, fabric, bridge] = adapter();
  bridge.rejection = true;
  fabric.queue.push({
    claimed: true,
    replayed: false,
    claim_token: "claim",
    delivery: delivery(),
    message: message(),
  });
  await subject.start();
  await subject.tick();
  assert.ok(fabric.calls.includes("fail:d-1"));
  bridge.rejection = false;
  bridge.throwDelivery = true;
  bridge.receipts.set(nativeDeliveryId("d-2"), {
    status: "accepted",
    revision: 1,
    request: {},
  } as AgentMessageDeliveryReceipt);
  fabric.queue.push({
    claimed: true,
    replayed: false,
    claim_token: "claim-2",
    delivery: delivery("d-2"),
    message: message("m-2"),
  });
  await subject.tick();
  assert.ok(fabric.calls.includes("ack:d-2"));
  await subject.stop();
});

test("a returned native rejection fails the fabric row even when native readback is unavailable", async () => {
  const [subject, fabric, bridge] = adapter();
  bridge.rejection = true;
  bridge.readbackFailures = 1;
  fabric.queue.push({
    claimed: true,
    replayed: false,
    claim_token: "claim",
    delivery: delivery(),
    message: message(),
  });
  await subject.start();
  await subject.tick();
  assert.ok(fabric.calls.includes("fail:d-1"));
  await subject.stop();
});

test("startup reconciliation settles a post-begin row by exact native readback", async () => {
  const [subject, fabric, bridge] = adapter();
  fabric.dispatching.push({
    ...delivery("d-restart"),
    state: "dispatching",
    native_attempt_ref: nativeAttemptRef("d-restart"),
  });
  bridge.receipts.set(nativeDeliveryId("d-restart"), {
    status: "accepted",
    revision: 1,
    request: {},
  } as AgentMessageDeliveryReceipt);
  await subject.start();
  assert.ok(fabric.calls.includes("ack:d-restart"));
  await subject.stop();
});

test("lease renewal keeps the stable adapter identity while fencing with the new token", async () => {
  const [subject, fabric] = adapter(undefined, undefined, { renewMs: 0 });
  await subject.start();
  await subject.tick();
  assert.ok(fabric.registerCount >= 2);
  await subject.stop();
});

test("shutdown waits for a begun dispatch and does not release it as a pre-begin claim", async () => {
  const [subject, fabric] = adapter();
  let open!: () => void;
  fabric.beginGate = new Promise<void>((resolve) => {
    open = resolve;
  });
  fabric.queue.push({
    claimed: true,
    replayed: false,
    claim_token: "claim",
    delivery: delivery(),
    message: message(),
  });
  await subject.start();
  const ticking = subject.tick();
  while (!fabric.calls.includes("begin:d-1"))
    await new Promise((resolve) => setTimeout(resolve, 0));
  const stopping = subject.stop();
  open();
  await Promise.all([ticking, stopping]);
  assert.ok(fabric.calls.includes("ack:d-1"));
  assert.ok(!fabric.calls.includes("release:d-1"));
});

test("an ambiguous committed begin survives failed readback and is reconciled on the next adapter start", async () => {
  const [subject, fabric, bridge] = adapter();
  fabric.throwAfterBegin = true;
  fabric.getDeliveryFailures = 1;
  fabric.queue.push({
    claimed: true,
    replayed: false,
    claim_token: "claim",
    delivery: delivery(),
    message: message(),
  });
  await subject.start();
  await subject.tick();
  await subject.stop();
  assert.ok(fabric.calls.includes("begin:d-1"));
  assert.ok(!fabric.calls.includes("release:d-1"));
  bridge.receipts.set(nativeDeliveryId("d-1"), {
    status: "accepted",
    revision: 1,
    request: {},
  } as AgentMessageDeliveryReceipt);
  const [restarted] = adapter(fabric, bridge);
  await restarted.start();
  assert.ok(fabric.calls.includes("ack:d-1"));
  await restarted.stop();
});
