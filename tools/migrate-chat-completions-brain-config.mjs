#!/usr/bin/env node

import { readFile, readdir, rename, unlink, writeFile } from "node:fs/promises";
import { basename, join, resolve } from "node:path";

const OLD_MODULE_ID = "pi-agent";
const NEW_MODULE_ID = "chat-completions";

const roots = process.argv.slice(2);
if (roots.length === 0) {
  console.error(
    "usage: node tools/migrate-chat-completions-brain-config.mjs <service-root> [...]",
  );
  process.exitCode = 2;
} else {
  const results = [];
  for (const root of roots) {
    results.push(await migrateServiceRoot(resolve(root)));
  }
  console.log(
    JSON.stringify(
      { oldModuleId: OLD_MODULE_ID, newModuleId: NEW_MODULE_ID, results },
      null,
      2,
    ),
  );
}

async function migrateServiceRoot(root) {
  const profilesDir = join(root, "config", "profiles");
  const entries = await readdir(profilesDir, { withFileTypes: true });
  const files = entries
    .filter((entry) => entry.isFile() && entry.name.endsWith(".json"))
    .map((entry) => join(profilesDir, entry.name))
    .sort();
  const migrated = [];

  for (const file of files) {
    const raw = await readFile(file, "utf8");
    const profile = JSON.parse(raw);
    if (profile?.brain?.module !== OLD_MODULE_ID) continue;
    profile.brain.module = NEW_MODULE_ID;
    await atomicJsonWrite(file, profile);
    migrated.push(basename(file));
  }

  for (const file of files) {
    const profile = JSON.parse(await readFile(file, "utf8"));
    if (profile?.brain?.module === OLD_MODULE_ID) {
      throw new Error(`brain identity migration did not update ${file}`);
    }
  }

  return { root, scanned: files.length, migrated };
}

async function atomicJsonWrite(path, value) {
  const temporaryPath = `${path}.chat-completions-migration-${process.pid}.tmp`;
  try {
    await writeFile(temporaryPath, `${JSON.stringify(value, null, 2)}\n`, {
      encoding: "utf8",
      mode: 0o600,
      flag: "wx",
    });
    await rename(temporaryPath, path);
  } catch (error) {
    await unlink(temporaryPath).catch(() => undefined);
    throw error;
  }
}
