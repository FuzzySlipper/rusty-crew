# Task 5895 Shared Credential Admin Live Certification

Date: 2026-07-16 (America/Los_Angeles)

## Target

- Service: `rusty-crew-debug.service`
- API: `http://127.0.0.1:9348`
- Deployment role: `debug`
- Storage: dedicated SQLite database under `/home/system/rusty-crew-debug`

The service was restarted from the task checkout after the native release build.
`GET /v1/admin/healthz` returned `health: ok` before the scenario ran.

## Scenario

The existing redacted OpenAI OAuth credential `provider:gpt-5.6-sol` was used
without reauthentication or secret readback.

1. Read the credential and the unlinked compatible Responses alias
   `gpt-5.6-terra`.
2. Linked that alias with explicit provider and credential revisions.
3. Read credential impact and observed two linked aliases.
4. Attempted credential clear and delete. Both returned HTTP 409 with
   `service_credential_linked`.
5. Unlinked `gpt-5.6-terra` with its new provider revision and confirmed the
   alias again had no credential.
6. Read credential-scoped OpenAI OAuth status and alias-scoped compatibility
   status. Both reported `hasSecret: true` without returning token material.

The temporary link was removed at the end. The credential remained linked only
to its original `gpt-5.6-sol` alias.

## Two-Alias Execution Extension

The rereview certificate extended the scenario from administrative linkage to
real provider execution. The existing OAuth credential was linked to
`gpt-5.6-terra` again without starting or completing another OAuth flow. Two
disposable profiles were then created, one for each provider alias.

The ChatGPT Codex endpoint does not accept the optional `max_output_tokens`
field, so that optional setting was omitted from both provider records before
the turns. The provider aliases, model IDs, endpoint, and shared credential
identity were otherwise unchanged.

Both profiles executed a real OpenAI Responses turn through the debug service:

| Provider alias | Wake ID | Model output | Terminal result |
| --- | --- | --- | --- |
| `gpt-5.6-sol` | `service-task5895-6-sol-1784277957-session-1784278324941-1` | `SHARED_CREDENTIAL_SOL_OK` | `assistant_message_completed`, `status=completed` |
| `gpt-5.6-terra` | `service-task5895-6-terra-1784277957-session-1784278326455-2` | `SHARED_CREDENTIAL_TERRA_OK` | `assistant_message_completed`, `status=completed` |

Each wake emitted a provider-request debug reference before streaming its exact
marker. Redacted credential readback during the executions reported credential
revision `1`, `hasSecret: true`, and both linked aliases. The serialized API
response contained none of `accessToken`, `refreshToken`, `idToken`, or
`exchangedApiToken`.

After the proof, both disposable profiles and their sessions were hard-deleted.
The temporary Terra link was removed at provider revision `9`; credential
readback again showed only `gpt-5.6-sol` linked, and both disposable profile
registry reads returned HTTP 404.

## Result

```text
shared credential debug certification passed
credential=provider:gpt-5.6-sol
temporaryAlias=gpt-5.6-terra
linkedAliasCount=2
clearGuard=409
deleteGuard=409
oauthStatusRedacted=true
restoredAliasUnlinked=true
solExecution=completed:SHARED_CREDENTIAL_SOL_OK
terraExecution=completed:SHARED_CREDENTIAL_TERRA_OK
sharedCredentialRevision=1
tokenMaterialReturned=false
```

The alias compatibility facade also returned:

```json
{
  "ok": true,
  "compatibilityFacade": true,
  "providerAlias": "gpt-5.6-sol",
  "serviceCredentialId": "provider:gpt-5.6-sol",
  "hasSecret": true,
  "pendingCount": 0
}
```
