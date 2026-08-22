import assert from "node:assert/strict";
import { createCrewServicesToolResolver } from "../src/crew-services-tools.js";

const resolver = createCrewServicesToolResolver({
  available: (sessionId) => sessionId === "bound-session",
  async directory(sessionId) {
    assert.equal(sessionId, "bound-session");
    return [{ alias: "beta", routeRevision: 2 }];
  },
  async message(input) {
    assert.equal(input.sessionId, "bound-session");
    assert.equal(input.toolCallId, "call-1");
    return { messageId: "m-1", replayed: false };
  },
});
assert.deepEqual(createCrewServicesToolResolver(undefined)({} as never), []);
assert.deepEqual(
  resolver({ wake: { state: { session: { sessionId: "unbound" } } } } as never),
  [],
);
const [directory, message] = resolver({
  wake: { state: { session: { sessionId: "bound-session" } } },
} as never);
assert.equal(directory?.name, "crew_directory");
const context = { sessionId: "bound-session", callId: "call-1" } as never;
const listed = await directory!.executeWithContext!({}, context);
assert.match(listed.content[0]?.type === "text" ? listed.content[0].text : "", /beta/);
const sent = await message!.executeWithContext!(
  { recipientAlias: "beta", body: "hello", replyToMessageId: "m-parent" },
  context,
);
assert.match(sent.content[0]?.type === "text" ? sent.content[0].text : "", /m-1/);
const missing = await message!.execute!("call-1", { recipientAlias: "beta", body: "hello" });
assert.match(missing.content[0]?.type === "text" ? missing.content[0].text : "", /tool_context_required/);
