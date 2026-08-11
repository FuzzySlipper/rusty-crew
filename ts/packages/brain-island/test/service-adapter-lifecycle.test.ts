import assert from "node:assert/strict";
import test from "node:test";
import { telegramBotTokenFromServiceCredentialSecret } from "../src/service-adapter-lifecycle.js";

test("Telegram connector unwraps persisted API-key credential envelopes", () => {
  assert.equal(
    telegramBotTokenFromServiceCredentialSecret(
      JSON.stringify({ kind: "api_key", version: 1, value: "bot-token" }),
    ),
    "bot-token",
  );
});

test("Telegram connector preserves legacy raw credentials", () => {
  assert.equal(
    telegramBotTokenFromServiceCredentialSecret("  legacy-bot-token  "),
    "legacy-bot-token",
  );
});

test("Telegram connector rejects incompatible credential envelopes without exposing secrets", () => {
  const secret = "must-not-appear";
  assert.throws(
    () =>
      telegramBotTokenFromServiceCredentialSecret(
        JSON.stringify({
          kind: "openai_oauth",
          version: 1,
          access_token: secret,
        }),
      ),
    (error: unknown) => {
      assert.ok(error instanceof Error);
      assert.match(error.message, /version 1 API-key secret/u);
      assert.doesNotMatch(error.message, new RegExp(secret, "u"));
      return true;
    },
  );
});

test("Telegram connector rejects malformed JSON envelopes without exposing input", () => {
  const secret = '{"kind":"api_key","value":"must-not-appear"';
  assert.throws(
    () => telegramBotTokenFromServiceCredentialSecret(secret),
    (error: unknown) => {
      assert.ok(error instanceof Error);
      assert.match(error.message, /secret envelope is invalid/u);
      assert.doesNotMatch(error.message, /must-not-appear/u);
      return true;
    },
  );
});
