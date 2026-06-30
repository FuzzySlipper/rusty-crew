import assert from "node:assert/strict";
import {
  routeSlashCommand,
  type SlashCommandRouteResult,
  type SlashCommandSession,
} from "./index.js";

const primeSession: SlashCommandSession = {
  sessionId: "session-alpha",
  agentId: "agent-alpha",
  profileId: "prime",
  kind: "full",
};
const workerSession: SlashCommandSession = {
  sessionId: "session-worker",
  agentId: "agent-worker",
  profileId: "coder",
  kind: "worker",
};

const passThrough = routeSlashCommand(input("hello there", primeSession));
assert.equal(passThrough.kind, "pass_through");

const help = intercepted(routeSlashCommand(input("/help", primeSession)));
assert.equal(help.commandName, "help");
assert.equal(help.status, "ok");
assert.equal(help.response.items?.includes("/new"), true);

const status = intercepted(routeSlashCommand(input("/status", primeSession)));
assert.equal(status.commandName, "status");
assert.equal(status.controlRequest, undefined);
assert.equal(status.response.fields?.sessionId, "session-alpha");

const session = intercepted(routeSlashCommand(input("/session", primeSession)));
assert.equal(session.commandName, "session");
assert.equal(session.response.fields?.profileId, "prime");

const model = intercepted(routeSlashCommand(input("/model", primeSession)));
assert.equal(model.commandName, "model");
assert.equal(model.status, "ok");
assert.equal(model.controlRequest, undefined);

const newSession = intercepted(
  routeSlashCommand(input("/new fresh start", primeSession)),
);
assert.equal(newSession.commandName, "new");
assert.equal(newSession.controlRequest?.commandName, "new_session");
assert.equal(newSession.controlRequest?.target.sessionId, "session-alpha");
assert.equal(newSession.controlRequest?.reason, "fresh start");
assert.equal(newSession.controlRequest?.body.actorId, "human-alpha");

const reloadMcp = intercepted(
  routeSlashCommand(input("/reload-mcp catalog refresh", primeSession)),
);
assert.equal(reloadMcp.commandName, "reload-mcp");
assert.equal(reloadMcp.controlRequest?.commandName, "reload_mcp");
assert.equal(reloadMcp.controlRequest?.reasonCode, "slash_reload_mcp");

const deniedWorkerControl = intercepted(
  routeSlashCommand(input("/new", workerSession)),
);
assert.equal(deniedWorkerControl.status, "denied");
assert.equal(deniedWorkerControl.controlRequest, undefined);

const allowedWorkerControl = intercepted(
  routeSlashCommand(
    input("/new", workerSession, { allowWorkerControls: true }),
  ),
);
assert.equal(allowedWorkerControl.status, "ok");
assert.equal(allowedWorkerControl.controlRequest?.commandName, "new_session");

const deniedRead = intercepted(
  routeSlashCommand(input("/status", workerSession)),
);
assert.equal(deniedRead.status, "denied");

const allowedRead = intercepted(
  routeSlashCommand(
    input("/model", workerSession, { allowNonPrimeReadCommands: true }),
  ),
);
assert.equal(allowedRead.status, "ok");

const unknown = intercepted(routeSlashCommand(input("/whoops", primeSession)));
assert.equal(unknown.commandName, "unknown");
assert.equal(unknown.status, "invalid");
assert.equal(unknown.response.items?.includes("/help"), true);

const limited = intercepted(
  routeSlashCommand(
    input("/new", primeSession, { allowedCommands: ["help", "status"] }),
  ),
);
assert.equal(limited.status, "denied");
assert.equal(limited.controlRequest, undefined);

console.log(
  JSON.stringify(
    {
      passThrough: passThrough.kind,
      help: help.response.items?.length,
      model: model.commandName,
      newControl: newSession.controlRequest?.commandName,
      reloadControl: reloadMcp.controlRequest?.commandName,
      workerDenied: deniedWorkerControl.status,
      unknown: unknown.commandName,
      limited: limited.status,
    },
    null,
    2,
  ),
);

function input(
  text: string,
  session: SlashCommandSession,
  options?: Parameters<typeof routeSlashCommand>[0]["options"],
): Parameters<typeof routeSlashCommand>[0] {
  return {
    text,
    session,
    actor: { id: "human-alpha" },
    source: {
      adapterId: "den-channels",
      bindingId: "binding-alpha",
      channelId: "crew-room",
      messageId: "message-alpha",
    },
    options,
  };
}

function intercepted(
  result: SlashCommandRouteResult,
): Extract<SlashCommandRouteResult, { kind: "intercepted" }> {
  assert.equal(result.kind, "intercepted");
  if (result.kind !== "intercepted") throw new Error("expected interception");
  return result;
}
