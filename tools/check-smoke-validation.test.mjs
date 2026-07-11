import assert from "node:assert/strict";
import test from "node:test";

import {
  auditSmokeValidation,
  extractSmokeScriptNames,
} from "./check-smoke-validation.mjs";

test("extracts smoke script names from chained verify commands", () => {
  assert.deepEqual(
    extractSmokeScriptNames(
      "npm run typecheck && npm run smoke:architecture-boundaries && npm run smoke:bridge-validation",
    ),
    ["architecture-boundaries", "bridge-validation"],
  );
});

test("accepts offline and native-offline verify smokes", () => {
  const audit = auditSmokeValidation({
    packageJson: {
      scripts: {
        "verify:ts":
          "npm run smoke:architecture-boundaries && npm run smoke:bridge-validation",
      },
    },
    catalog: [
      {
        name: "architecture-boundaries",
        scope: "root-alias",
        lane: "offline",
        category: "package-integration",
        requirements: ["none"],
      },
      {
        name: "bridge-validation",
        scope: "root-alias",
        lane: "native-offline",
        category: "native-bridge",
        requirements: ["native-build"],
      },
    ],
  });

  assert.equal(audit.violations.length, 0);
});

test("rejects live or infrastructure smokes in verify:ts", () => {
  const audit = auditSmokeValidation({
    packageJson: {
      scripts: {
        "verify:ts":
          "npm run smoke:telegram-live && npm run smoke:den-successor-service",
      },
    },
    catalog: [
      {
        name: "telegram-live",
        scope: "root-alias",
        lane: "live-provider",
        category: "adapter-integration",
        requirements: ["live-provider", "telegram-config"],
      },
      {
        name: "den-successor-service",
        scope: "root-alias",
        lane: "local-infrastructure",
        category: "den-adapter",
        requirements: ["den"],
      },
    ],
  });

  assert.equal(audit.violations.length, 4);
  assert.match(audit.violations[0]?.reason ?? "", /lane live-provider/);
  assert.match(audit.violations[1]?.reason ?? "", /live-provider/);
  assert.match(audit.violations[2]?.reason ?? "", /lane local-infrastructure/);
  assert.match(audit.violations[3]?.reason ?? "", /den/);
});

test("rejects new root smoke aliases above the frozen ceiling", () => {
  const catalog = Array.from({ length: 132 }, (_, index) => ({
    name: `root-${index}`,
    scope: "root-alias",
    lane: "offline",
    category: "package-integration",
    requirements: ["none"],
  }));

  const audit = auditSmokeValidation({
    packageJson: {
      scripts: {
        "verify:ts": "",
      },
    },
    catalog,
  });

  assert.equal(audit.violations.length, 1);
  assert.match(audit.violations[0]?.reason ?? "", /root package exposes 132/);
});

test("requires one top-level native build and a prebuilt native surface smoke", () => {
  const accepted = auditSmokeValidation({
    packageJson: {
      scripts: {
        "verify:ts":
          "npm run build:native && npm run smoke:bridge-native-surface",
        "smoke:bridge-native-surface": "node check-native-surface.mjs",
      },
    },
    catalog: [
      {
        name: "bridge-native-surface",
        scope: "root-alias",
        lane: "native-offline",
        category: "native-bridge",
        requirements: ["native-build"],
      },
    ],
  });
  assert.equal(accepted.nativeBuildInvocations, 1);
  assert.deepEqual(accepted.violations, []);

  const duplicate = auditSmokeValidation({
    packageJson: {
      scripts: {
        "verify:ts":
          "npm run build:native && npm run smoke:bridge-native-surface",
        "smoke:bridge-native-surface":
          "npm run build:native && node check-native-surface.mjs",
      },
    },
    catalog: [
      {
        name: "bridge-native-surface",
        scope: "root-alias",
        lane: "native-offline",
        category: "native-bridge",
        requirements: ["native-build"],
      },
    ],
  });
  assert.equal(duplicate.violations.length, 1);
  assert.match(duplicate.violations[0]?.reason ?? "", /without rebuilding/);
});
