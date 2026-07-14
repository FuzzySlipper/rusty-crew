import assert from "node:assert/strict";
import test from "node:test";

import { providerRequestTimeoutMs } from "../src/provider-request-timeout.js";

const parsers = [
  {
    name: "pi-agent",
    variable: "RUSTY_CREW_PI_AGENT_PROVIDER_REQUEST_TIMEOUT_MS",
    parse: (env: Partial<NodeJS.ProcessEnv>) =>
      providerRequestTimeoutMs("pi-agent", env),
  },
  {
    name: "openai-responses",
    variable: "RUSTY_CREW_OPENAI_RESPONSES_PROVIDER_REQUEST_TIMEOUT_MS",
    parse: (env: Partial<NodeJS.ProcessEnv>) =>
      providerRequestTimeoutMs("openai-responses", env),
  },
] as const;

for (const parser of parsers) {
  test(`${parser.name} provider request timeout is disabled by default`, () => {
    assert.equal(parser.parse({}), undefined);
  });

  test(`${parser.name} provider request timeout accepts explicit disabling`, () => {
    for (const value of ["", "0", "disabled", "none", " DISABLED "]) {
      assert.equal(parser.parse({ [parser.variable]: value }), undefined);
    }
  });

  test(`${parser.name} provider request timeout accepts a positive duration`, () => {
    assert.equal(parser.parse({ [parser.variable]: " 45000 " }), 45_000);
  });

  test(`${parser.name} provider request timeout rejects invalid values`, () => {
    for (const value of ["-1", "1.5", "soon", "9007199254740992"]) {
      assert.throws(
        () => parser.parse({ [parser.variable]: value }),
        new RegExp(parser.variable),
      );
    }
  });
}
