# Durable demand, coordination and recovery

This reference describes coordinator-owned workflows. For delegated roles, apply the current protocol:
{% include "protocol.md" %}

The coordinator owns acceptance, checkpoints and lifecycle. Read the matching Session, its current progress/history and selected Tasks before deciding what to do. Continue one evolving demand across interruptions, changed scope, turns and compaction. A different demand gets a new Session; a closed record is historical evidence, not something to reopen by inventing an operation.

## Establish the demand

With no matching nonterminal Session, use the advertised `{{ mcp_tool_task_session_create }}` schema. Preserve purpose, requested outcomes/examples, boundaries, accepted constraints, evidence, open questions, authorization and observable finish in its description. Strategy records investigation, ordering, ownership and the next decision. Bind subsequent Task creates to the actual returned `result.id`.

This fictional call illustrates the native shape. Its IDs are synthetic DATA, not authority or saved records. Replace selection with real captured values. A successful parser or example is not execution evidence.

~~~json
{
  "name": "{{ mcp_tool_task_session_create }}",
  "arguments": {
    "selection": {"project_id": "00000000000000000000000002", "project_version": "main"},
    "input": {
      "idempotency_key": "archive-demand",
      "title": "Deliver authorized archive browsing",
      "description": "R1: preserve the active-only default. R2: let authorized readers include archived projects without exposing inaccessible rows or counts. R3: document selection and failure recovery. Permissions and archive mutation remain outside scope.",
      "strategy": "Read current handler/query/paging and user/API documentation; settle presence and authorization contracts, save producer/UI/integration leaves with typed checks, and read back coverage before implementation."
    }
  }
}
~~~

Creation returns a Session record under `kind:"session",result`. Claim its returned ID using `input={id,lease_ms}`; `lease_ms` is a positive caller-requested duration in milliseconds, not a new product default. Claim returns `kind:"session_claim",result:{record,token}`. Retain the real token privately. The Task claim is separate; a Task token cannot mutate a Session.

## Current intent and checkpoints

Recover the initial request plus latest current Session checkpoint and linked Task evidence. Preserve current authorization before work; planning permission does not imply implementation or publishing permission. If scope changes, state the new requirements, retired obligations and rationale, source evidence and affected Task/check IDs before dependent work.

Use the Session checkpoint's actual schema: `input.target` carries the ID and private claim fence; `lease_ms`, `summary` and `next_step` are required. Summary can contain current intent/strategy, result/evidence, problems, decisions/rationale and ordering. Do not send separate `problems`, `decisions` or `strategy` fields that the checkpoint schema does not advertise. Read back the resulting Session revision/progress. A checkpoint does not revise Task specifications.

Before executing changed scope, use `{{ mcp_tool_task_session_revise }}` with the current Session target/fence, positive `lease_ms`, canonical `reason` and only the replacement `title`, `description` or `strategy`. Omission preserves each field; null and empty or padded text are invalid. The actual coordinator needs CoordinateTask admission and a live claim. A change must affect specification or authored references; a no-op is rejected without renewal. Revision preserves Creation, checkpoints and linked Tasks, and records immutable before/after snapshots. Re-read after intervening mutations or stale revision; reconcile affected Task specifications before dependent work.

This fictional call uses synthetic IDs and a synthetic nonce solely as parser DATA. Use the actual captured selection, current revision and privately retained participant token when executing.

~~~json
{
  "name": "{{ mcp_tool_task_session_revise }}",
  "arguments": {
    "selection": {"project_id": "00000000000000000000000002", "project_version": "main"},
    "input": {
      "target": {"id": "00000000000000000000000003", "claim": {"token": "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE", "expected_revision": 3}},
      "lease_ms": 30000,
      "reason": "The accepted demand now includes CSV export",
      "strategy": "Settle the CSV contract and revise affected leaf checks before implementation."
    }
  }
}
~~~

Use only advertised operations. An absent capability or conflicting live owner requires coordination and real lifecycle recovery; never invent takeover, switch actor or fabricate a fence.

The coordinator follows its assigned Session coordination and one active Task per executing agent as an operational workflow. This is not a quota on the authenticated actor: distinct targets retain independent claims. Delegated roles do not acquire claims; they report evidence or a draft for a write requiring ownership. A conflicting live owner, stale revision or expired lease requires genuine target lifecycle recovery, never identity spoofing or another target’s private token/fence.

## Handoff and finish

At meaningful outcomes checkpoint the Session with the work-unit evidence and exact continuation. Tokens never belong in descriptions, links, artifacts or a handoff. If stopping, release the coordinator's Task and Session separately using their current private fences and describe delivered, remaining, blocked and unrun work.

A hypothetical archive handoff states: R1/R2 API behavior is implemented and verified against the fixture; the UI retry guide and combined journey remain pending; the exact UI leaf/check IDs and source sections are named. It does not say the Session is done because the API leaf completed.

On resume, reread that evidence and current intent, acquire normal available claims, confirm revisions and prerequisite results, then continue. Before final closure reconcile scope changes, actual Task states, integrated behavior, documentation and justified cancellations. `{{ mcp_tool_task_session_complete }}` takes the real Session ID and claim as its operation input. Confirm the result; stop hooks and lease expiry cannot complete the user's demand.
