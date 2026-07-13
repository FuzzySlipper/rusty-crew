import { runCoordinationOperatorCli } from "./coordination-operator-cli.js";

await runCoordinationOperatorCli("debug", process.argv.slice(2));
