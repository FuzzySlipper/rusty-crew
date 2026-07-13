import { runCoordinationOperatorCli } from "./coordination-operator-cli.js";

await runCoordinationOperatorCli("production", process.argv.slice(2));
