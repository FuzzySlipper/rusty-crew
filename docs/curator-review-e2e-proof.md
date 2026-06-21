# Curator And Background Review E2E Proof

Task: `2984`

## Purpose

`smoke:curator-review-e2e` proves that curator candidate/reporting work,
dry-run mutation preview, static background review, diagnostics, and governance
observation can be used together without treating channels or Den projection as
coordination authority.

## Covered Behavior

- Curator discovery produces a deterministic candidate batch and rendered report
  from skills plus dense profile memory.
- A curator mutation candidate can be previewed through the governance executor
  with `dryRun: true`.
- The preview leaves the source skill unchanged and writes no mutation records.
- Static background memory/skill review produces bounded findings and records
  skipped LLM review because provider-backed review must use the normal
  brain/provider path.
- Background governance observation records the curator loop result.
- Admin background diagnostics expose curator candidate counts, mutation counts,
  and recent background-review findings.

## Verification

Run:

```bash
npm run smoke:curator-review-e2e
```

Expected proof points:

- curator candidates are greater than zero;
- preview status is `previewed`;
- mutation count remains `0`;
- review findings are greater than zero;
- background diagnostics health is `ok`;
- observation events include background review checkpoint and curator
  completion.
