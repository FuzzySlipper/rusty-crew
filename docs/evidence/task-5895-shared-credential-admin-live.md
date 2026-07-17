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
