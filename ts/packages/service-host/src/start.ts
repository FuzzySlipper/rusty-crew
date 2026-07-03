import { fileURLToPath } from "node:url";

import { startRustyCrewServiceHost } from "./index.js";

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}

async function main(): Promise<void> {
  const host = await startRustyCrewServiceHost();
  console.log(`rusty-crew service listening on ${host.url}`);
  const shutdown = () => {
    void host.stop().finally(() => process.exit(0));
  };
  process.once("SIGINT", shutdown);
  process.once("SIGTERM", shutdown);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  void main().catch((error) => {
    console.error(errorMessage(error, "rusty-crew service failed"));
    process.exit(1);
  });
}
