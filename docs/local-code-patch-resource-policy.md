# Local Code And Patch Resource Policy

Status: active design note for task 5309

Rust owns reusable local-code and patch resource facts. TypeScript owns the
execution mechanics that actually touch the filesystem, spawn processes, and
apply patches.

## Rust-Owned Facts

- default service-agent workdir: `/home`
- session workdir normalization and command duration ceiling normalization
- read, search-file, and command-output byte caps
- tool identity for local-code and patch tools
- filesystem scope per tool: unrestricted for full-agent local tools, workdir
  scoped for `worker_write` and `worker_patch`
- process execution flag, write flag, execution mode, and telemetry output shape
  per tool
- shared denial reason vocabulary for path escape, read/stat/write failures,
  file-too-large skips, command failures/timeouts, patch parse/match failures,
  and syntax rollback failures

## TypeScript-Owned Mechanics

- resolving paths with Node path APIs
- reading, writing, and searching files
- spawning shell and git processes
- parsing and applying V4A patch blocks
- running syntax checks and rollback writes
- returning concrete tool call content and details to the brain harness

The TypeScript wrappers consume a `NativeLocalCodeResourcePolicyPlan` object.
The running service asks the native bridge for the canonical Rust plan during
runtime config apply and brain rebuild. Package-level fallback use still has a
matching default DTO so offline smokes can run without a loaded native addon.
