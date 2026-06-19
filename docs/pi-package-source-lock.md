# pi Package Source Lock

Rusty-Crew's TypeScript brain island uses the current `earendil-works/pi`
packages as the upstream Agent/LLM island.

## Current Pin

- Repository: `https://github.com/earendil-works/pi`
- Commit: `6e6ce70caf3328683517b0e308fdbbc6d1c1abc9`
- `@earendil-works/pi-agent-core`: `0.79.8`
- `@earendil-works/pi-ai`: `0.79.8`

The npm package metadata for both packages points back to this repository under
`packages/agent` and `packages/ai`. The scaffold consumes exact npm versions so
normal installs do not depend on a floating Git branch.

## Update Procedure

1. Run `git ls-remote https://github.com/earendil-works/pi.git HEAD`.
2. Clone or fetch that commit and read `packages/agent/package.json` and
   `packages/ai/package.json`.
3. Update `ts/packages/brain-island/package.json` exact dependency versions and
   `piPackageSource.commit`.
4. Run `npm install`.
5. Run `npm run smoke:brain`, `npm run typecheck`, and `npm run format`.

Older local checkout paths in audit docs are historical context only.
