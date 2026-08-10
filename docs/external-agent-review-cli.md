# External Agent Review CLI

`tools/rusty-crew-review.mjs` is the unmanaged-agent path for submitting an
exact commit to the same durable Rusty Crew review workflow used by native Crew
brains. It is a Node-only CLI: it does not require Den MCP discovery, a Crew
brain session, or a fake session created for the external agent.

The canonical submitter/reviewer decision tree and recovery contract is Den
document `den-services/review-pointer-first-contract`. This guide is the
external CLI specialization of that contract.

## Deployment Selection

Always provide both the service URL and its expected deployment role. The role
check is intentional: it prevents a debug review from silently going to the
long-lived service, or a production review from being sent to the disposable
service.

| Deployment | Default URL | Role | Storage |
| --- | --- | --- | --- |
| Long-lived service | `http://127.0.0.1:9347` | `production` | PostgreSQL |
| Disposable test service | `http://127.0.0.1:9348` | `debug` | SQLite |

LAN and reverse-proxy URLs are fine, but the role flag is still required. The
CLI does not infer a deployment from a port, hostname, or a mutable profile.

The selected service must have these review settings:

```dotenv
RUSTY_CREW_REVIEW_DEN_AUTHORITY_ID=service-review-den
RUSTY_CREW_REVIEW_DEN_ENDPOINT_REF=config://mcp/den
RUSTY_CREW_REVIEW_DEN_AUDIT_IDENTITY=rusty-crew-review-service
# Optional when the Den MCP endpoint authenticates callers:
RUSTY_CREW_REVIEW_DEN_BEARER_TOKEN=<dedicated review automation credential>
```

The authority is service-owned configuration outside the runtime binding
graph. It must not depend on the external caller having a Crew profile or
session. The existing
`RUSTY_CREW_REVIEW_URL`, `RUSTY_CREW_REVIEW_BEARER_TOKEN`, and GitHub gate
consumer settings remain responsible for the Den Review/GitHub event adapter.

### Migration from a session binding

Remove `RUSTY_CREW_REVIEW_DEN_BINDING_ID`; do not restore or preserve the old
binding in `service.json`. Configure the authority variables above in the
service environment, restart the service, and read
`GET /v1/admin/diagnostics/review-den-authority`. The status must be `ready`,
the server must be `den`, and `missingTools` must be empty. Startup
reconciliation resumes existing durable submissions, including a round that
Den finalized while Crew was unable to call it; do not submit a replacement
review or change its idempotency key.

Set `RUSTY_CREW_ADMIN_TOKEN` in the shell when the selected service uses
bearer authentication. Do not put the token in a task, review summary,
repository, shell history, or this document.

## Install Or Run

The script uses only Node's built-in modules and `fetch`:

```bash
# From a Rusty Crew checkout:
node tools/rusty-crew-review.mjs --help

# Optional user-local command name:
install -Dm755 tools/rusty-crew-review.mjs ~/.local/bin/rusty-crew-review
```

The repository convenience form is also available from the checkout:

```bash
npm run review:cli -- --help
```

An external repository can use the installed command or invoke the script by
absolute path. No Rusty Crew branch, working directory, MCP server, or model
provider is required in the external repository.

## Submit

Submit the exact 40-character commit SHA, not a branch name or a short SHA:

```bash
export RUSTY_CREW_ADMIN_TOKEN='...'
rusty-crew-review submit \
  --service-url http://127.0.0.1:9347 \
  --deployment-role production \
  --project-id rusty-crew \
  --task 6644 \
  --repository FuzzySlipper/rusty-crew \
  --sha 0123456789abcdef0123456789abcdef01234567 \
  --ref main \
  --check 'Verify Offline' \
  --check 'Verify Postgres Backend' \
  --base-sha fedcba9876543210fedcba9876543210fedcba98 \
  --summary-file review-summary.md \
  --client-id external-codex \
  --idempotency-key task-6644-01234567
```

Use `--summary` for a short inline summary or `--summary-file` for the normal
markdown handoff. The caller supplies the Den project for each review; Crew
does not keep a project allowlist. The service validates the repository, SHA,
task, checks, and idempotency identity, while Den remains authoritative for the
project and task.

The receiver is always `@reviewer`. There is deliberately no `--reviewer`
option and the HTTP API rejects a reviewer field in the external request.
After the exact-SHA GitHub gate passes, Crew sends one durable review request
to the configured reviewer route. The reviewer records the verdict and
findings in Den; there is no external-agent wake or reply attempt.

An identical task/SHA/client/idempotency submission is idempotent. Reusing the
same idempotency key with different review material is rejected. A gate failure
settles the task back to `in_progress` and does not dispatch `@reviewer`.

## Status And Waiting

Every submit response includes a `submissionId`. Read it later with an
explicitly role-bound status request:

```bash
rusty-crew-review status \
  --service-url http://127.0.0.1:9347 \
  --deployment-role production \
  --submission-id review-submission:<sha256-id>
```

To let the CLI poll until the review reaches a terminal phase:

```bash
rusty-crew-review status \
  --service-url http://127.0.0.1:9348 \
  --deployment-role debug \
  --submission-id review-submission:<sha256-id> \
  --wait \
  --poll-ms 5000 \
  --timeout-ms 600000
```

The example bounds the wait at ten minutes. On expiry the CLI reports the
review as still pending and exits with code `2`; that is not a failed review.
Choose a larger finite timeout when the repository's gate duration requires
it. `--timeout-ms` is optional, but unbounded `--wait` should be reserved for
an intentional operator session. Use `--json` for automation-friendly output.
Human output includes the
submission id, selected deployment, exact SHA, phase, gate state, verdict, and
durable adapter/terminal reasons when present.

The normal durable phase vocabulary is `gate_pending`,
`reviewer_dispatched`, `den_finalization_pending`, `reply_pending`, and
`review_terminal`. Accepted or pending state is never review completion.
Require terminal Crew status plus Den round/task readback before claiming the
review finished.

Exit codes:

| Code | Meaning |
| --- | --- |
| `0` | Accepted, or terminal `looks_good` review |
| `2` | Still pending, including an explicit wait timeout |
| `3` | GitHub gate failed, timed out, or was superseded by gate policy; inspect `gateStatus` and `terminalReason` |
| `4` | Reviewer requested changes or the reply path was terminal |
| `5` | This submission was superseded by a newer SHA for the task |
| `64` | CLI usage or input error |
| `70` | Service, authentication, or API error |

## HTTP Contract

The CLI uses authenticated admin routes:

```text
POST /v1/admin/review-submissions
GET  /v1/admin/review-submissions/{submission_id}?expectedDeploymentRole=production|debug
```

Both return the normal `{ ok, data, meta }` Rusty Crew envelope. The public
capability inventory and generated OpenAPI artifact expose these routes as
`review.submissions.external.create` and
`review.submissions.external.read`.

## Boundaries And Troubleshooting

- This path is for an unmanaged external CLI agent. Managed Crew brains should
  use the model-callable `submit_task_for_review` tool.
- This path does not call Den MCP from the external repository. Crew uses the
  explicit service Den binding to request the review round, register/watch the
  exact GitHub checks, finalize Den, and dispatch the fixed reviewer.
- The external caller is not a Rusty Crew agent, brain, or session. Its durable
  identity is `clientId` plus `idempotencyKey`.
- A response with `lastAdapterError` is durable pending state, not permission
  to retry with a new idempotency key. Inspect the error, fix service
  configuration, and poll the same submission.
- `deployment_role_mismatch` means the URL and role flag disagree. Check the
  URL, service port, and service environment before retrying.
- `review_den_authority_unavailable` means the dedicated service authority is
  unconfigured, unreachable, or missing one of the exact review workflow
  operations. No new external submission is durably admitted in this state;
  existing submissions remain pending and reconciliation retries after the
  authority is restored.
- A non-`passed` `gateStatus` means no reviewer request was sent. The durable
  record may settle into `review_terminal` after the task is reset; use
  `gateStatus` and `terminalReason`, not only `phase`, to classify that outcome.
  Fix the commit/check issue, push a new exact SHA, and submit a new idempotency key.
- A pending reviewer phase means the deterministic service workflow is waiting
  on the reviewer. Poll the same submission; do not create a second submission
  for the same exact work.
- Generic `send_agent_message`, `agent_round`, raw `reply_agent_message`, and
  Codex app thread steering are collaboration/diagnostic surfaces. None creates,
  completes, or repairs a managed review submission.
- After Crew persists a reviewer result, attempts Den finalization, or returns
  a missing/ambiguous completion receipt, reconcile the same submission and Den
  round. Do not issue a second completion or finalization. Only an explicit
  pre-persistence validation rejection that says no result was persisted permits
  correcting and repeating the managed completion.

The lower-level Den MCP review tools and the managed reviewer tool remain
available for their intended internal paths. This CLI is the single green path
for external agents that cannot call those tools directly.
