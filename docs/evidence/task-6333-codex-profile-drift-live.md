# Task 6333 Codex Profile Drift Live Certificate

Date: 2026-07-28

Service: debug SQLite deployment at `http://127.0.0.1:9348`

Runtime: `rv-live-codex-5516`

Isolated profile: `task-6333-live-cert`

Binding: `external-binding-c380810a15406e596529a2f0`

## Applied Prompt Continues During Drift

The binding was created at profile revision 1 with applied prompt marker
`TASK_6333_APPLIED_PROMPT_V1` and native thread
`019fa7da-e673-77e1-9fbc-8f37457aff51`.

The profile prompt was then changed to revision 2. The binding fleet read model
reported:

- state `stale` and `refreshRequired: true`;
- applied revision 1 and hash `a837daa59a81fc49f79a2a1349a3fe02cc7f2dab51eab6f7b410fe022d75983d`;
- current revision 2 and hash `29a436ce23b29a1c59445e6f26d9b3825cc2da97030448100f310277f9b69a05`.

An ordinary operator message completed successfully on the unchanged native
thread. The live Codex response was exactly:

```text
TASK_6333_APPLIED_PROMPT_V1
```

The external turn reached `completed` with no terminal reason code.

## Explicit Refresh

An initial fork experiment demonstrated that Codex fork history retains the
old profile collaboration instruction as conversation input even when the
fork's current settings contain the new developer instruction. That made a
fork an invalid refresh primitive. The implementation therefore creates a
clean replacement thread while preserving the previous thread as archived,
discoverable history.

After applying profile revision 3, the explicit refresh returned:

- outcome `thread_replaced`;
- the same binding, Crew session, agent, label, and cwd;
- previous thread `019fa7db-ecfc-7b40-a38d-57666a704e21`, archived;
- replacement thread `019fa7e3-6332-7990-b5a5-cb3c4c7d280c`;
- profile state `current`, revision 3, with matching applied/current hashes.

A live message on the replacement thread completed and answered exactly:

```text
TASK_6333_REFRESHED_PROMPT_V3
```

A binding identity comparison before and after refresh found no native-thread
or binding-revision changes among unrelated active bindings.

## Restart Coverage

The debug Crew service was restarted between refresh rounds. The task binding
hydrated on its persisted applied prompt snapshot and original bound thread;
restart did not adopt a newer profile prompt or reject the binding. Focused
controller tests additionally exercise this restart case with deterministic
transport assertions.
