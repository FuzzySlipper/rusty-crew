import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  realpathSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const packageRoot = join(root, "ts/packages/external-runtime-codex");
const expectedVersion =
  process.env.CODEX_APP_SERVER_PROTOCOL_VERSION ?? "0.144.1";
const protocolRoot = join(packageRoot, "protocol", expectedVersion);
const checkOnly = process.argv.includes("--check");
const checkRuntime = process.argv.includes("--runtime");
const codexCommand = process.env.CODEX_EXECUTABLE ?? "codex";

function sha256File(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function filesUnder(path) {
  const files = [];
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    const child = join(path, entry.name);
    if (entry.isDirectory()) files.push(...filesUnder(child));
    else if (entry.isFile()) files.push(child);
  }
  return files.sort();
}

function sha256Tree(path) {
  const hash = createHash("sha256");
  for (const file of filesUnder(path)) {
    hash.update(relative(path, file));
    hash.update("\0");
    hash.update(readFileSync(file));
    hash.update("\0");
  }
  return hash.digest("hex");
}

function canonicalJson(value) {
  if (Array.isArray(value)) return value.map(canonicalJson);
  if (value !== null && typeof value === "object") {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, canonicalJson(value[key])]),
    );
  }
  return value;
}

function sha256JsonTree(path) {
  const hash = createHash("sha256");
  for (const file of filesUnder(path)) {
    hash.update(relative(path, file));
    hash.update("\0");
    hash.update(JSON.stringify(canonicalJson(JSON.parse(readFileSync(file)))));
    hash.update("\0");
  }
  return hash.digest("hex");
}

function canonicalizeJsonTree(path) {
  for (const file of filesUnder(path)) {
    writeFileSync(
      file,
      `${JSON.stringify(canonicalJson(JSON.parse(readFileSync(file))), null, 2)}\n`,
    );
  }
}

function makeGeneratedTsNodeNextCompatible(path) {
  for (const file of filesUnder(path).filter((candidate) =>
    candidate.endsWith(".ts"),
  )) {
    const source = readFileSync(file, "utf8");
    const rewritten = source.replace(
      /(\bfrom\s+["'])(\.{1,2}\/[^"']+?)(["'])/g,
      (match, prefix, specifier, suffix) => {
        if (/\.(?:js|json|node)$/.test(specifier)) return match;
        const sourceTarget = resolve(dirname(file), specifier);
        const runtimeSpecifier =
          existsSync(sourceTarget) && statSync(sourceTarget).isDirectory()
            ? `${specifier}/index.js`
            : `${specifier}.js`;
        return `${prefix}${runtimeSpecifier}${suffix}`;
      },
    );
    if (rewritten !== source) writeFileSync(file, rewritten);
  }
}

function findNativeExecutable(launcherPath) {
  const codexPackageRoot = resolve(dirname(realpathSync(launcherPath)), "..");
  const platformPackage = {
    "linux:x64": "@openai/codex-linux-x64",
    "linux:arm64": "@openai/codex-linux-arm64",
    "darwin:x64": "@openai/codex-darwin-x64",
    "darwin:arm64": "@openai/codex-darwin-arm64",
    "win32:x64": "@openai/codex-win32-x64",
    "win32:arm64": "@openai/codex-win32-arm64",
  }[`${process.platform}:${process.arch}`];
  if (platformPackage === undefined) {
    throw new Error(
      `unsupported Codex platform ${process.platform}:${process.arch}`,
    );
  }
  const require = createRequire(join(codexPackageRoot, "package.json"));
  const platformRoot = dirname(
    require.resolve(`${platformPackage}/package.json`),
  );
  const candidates = filesUnder(join(platformRoot, "vendor")).filter(
    (path) =>
      (path.endsWith("/bin/codex") || path.endsWith("/bin/codex.exe")) &&
      statSync(path).size > 0,
  );
  if (candidates.length !== 1) {
    throw new Error(
      `expected one native Codex executable under ${platformRoot}, found ${candidates.length}`,
    );
  }
  return candidates[0];
}

function inspectRuntime() {
  const launcherPath = realpathSync(
    execFileSync("which", [codexCommand], { encoding: "utf8" }).trim(),
  );
  const versionOutput = execFileSync(codexCommand, ["--version"], {
    encoding: "utf8",
  }).trim();
  const match = /^codex-cli\s+(.+)$/.exec(versionOutput);
  if (match?.[1] !== expectedVersion) {
    throw new Error(
      `expected codex-cli ${expectedVersion}, observed ${versionOutput}`,
    );
  }
  const nativeExecutablePath = findNativeExecutable(launcherPath);
  return {
    cliVersion: expectedVersion,
    launcherSha256: sha256File(launcherPath),
    nativeExecutableSha256: sha256File(nativeExecutablePath),
  };
}

function generateInto(destination) {
  const tsOut = join(destination, "ts");
  const jsonOut = join(destination, "json");
  mkdirSync(tsOut, { recursive: true });
  mkdirSync(jsonOut, { recursive: true });
  const prettier = join(root, "node_modules/.bin/prettier");
  execFileSync(
    codexCommand,
    [
      "app-server",
      "generate-ts",
      "--experimental",
      "--out",
      tsOut,
      "--prettier",
      prettier,
    ],
    { stdio: "inherit" },
  );
  makeGeneratedTsNodeNextCompatible(tsOut);
  execFileSync(
    codexCommand,
    ["app-server", "generate-json-schema", "--experimental", "--out", jsonOut],
    { stdio: "inherit" },
  );
  canonicalizeJsonTree(jsonOut);
  const jsonFiles = filesUnder(jsonOut);
  for (let index = 0; index < jsonFiles.length; index += 100) {
    execFileSync(
      prettier,
      [
        "--write",
        "--log-level",
        "silent",
        ...jsonFiles.slice(index, index + 100),
      ],
      { stdio: "inherit" },
    );
  }
  return { tsOut, jsonOut };
}

function artifactIdentity(destination) {
  const tsSha256 = sha256Tree(join(destination, "ts"));
  const jsonSchemaSha256 = sha256JsonTree(join(destination, "json"));
  const protocolSchemaSha256 = createHash("sha256")
    .update(
      `codex-app-server\n${expectedVersion}\nexperimental=true\n${tsSha256}\n${jsonSchemaSha256}\n`,
    )
    .digest("hex");
  return { tsSha256, jsonSchemaSha256, protocolSchemaSha256 };
}

function readManifest() {
  return JSON.parse(readFileSync(join(protocolRoot, "manifest.json"), "utf8"));
}

if (checkOnly) {
  const manifest = readManifest();
  const identity = artifactIdentity(protocolRoot);
  for (const [key, value] of Object.entries(identity)) {
    if (manifest[key] !== value) {
      throw new Error(
        `Codex app-server generated artifact drift: ${key} expected ${manifest[key]}, observed ${value}`,
      );
    }
  }
  if (
    manifest.cliVersion !== expectedVersion ||
    manifest.experimental !== true
  ) {
    throw new Error("Codex app-server protocol manifest identity is invalid");
  }
  if (checkRuntime) {
    const scratch = join(
      tmpdir(),
      `rusty-crew-codex-protocol-${process.pid}-${Date.now()}`,
    );
    try {
      const runtime = inspectRuntime();
      generateInto(scratch);
      const regenerated = artifactIdentity(scratch);
      for (const [key, value] of Object.entries(regenerated)) {
        if (manifest[key] !== value) {
          throw new Error(
            `installed Codex protocol differs from committed artifact: ${key}`,
          );
        }
      }
      if (
        manifest.launcherSha256 !== runtime.launcherSha256 ||
        manifest.nativeExecutableSha256 !== runtime.nativeExecutableSha256
      ) {
        throw new Error(
          "installed Codex executable fingerprint is incompatible",
        );
      }
    } finally {
      rmSync(scratch, { recursive: true, force: true });
    }
  }
  console.log(
    `Codex app-server ${expectedVersion} generated protocol check passed${checkRuntime ? " against installed runtime" : ""}`,
  );
  process.exit(0);
}

if (existsSync(protocolRoot)) {
  rmSync(protocolRoot, { recursive: true, force: true });
}
mkdirSync(protocolRoot, { recursive: true });
const runtime = inspectRuntime();
generateInto(protocolRoot);
const identity = artifactIdentity(protocolRoot);
writeFileSync(
  join(protocolRoot, "manifest.json"),
  `${JSON.stringify(
    {
      protocol: "codex-app-server",
      cliVersion: expectedVersion,
      experimental: true,
      ...runtime,
      ...identity,
    },
    null,
    2,
  )}\n`,
);
console.log(
  `Generated Codex app-server ${expectedVersion} protocol at ${protocolRoot}`,
);
