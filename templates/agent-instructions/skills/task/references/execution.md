# Execute, revise, verify and hand off

This reference describes coordinator-owned workflows. For delegated roles, apply the current protocol:
{% include "protocol.md" %}

The coordinator owns acceptance, checks, checkpoints and lifecycle. Read [session.md](session.md) for current demand and [planning.md](planning.md) for a material contract change. The coordinator claims executable Tasks; delegated roles receive the bounded task and report findings without taking claims.

## Start a leaf

Read the leaf, parent requirement and named prerequisite results. Compare them with current AST/Knowledge and relevant decisions. Confirm file/interface ownership, readiness, real APIs, input/output/error/authority boundaries and practical verification. A path or completed badge alone does not prove that a dependency matches the saved plan.

Claim the actual ready Task through its advertised schema. The actor comes from authentication, not an input label. Keep its returned token and current specification revision privately; a claim is not a new permission. Keep one active Task per executing agent as an operational workflow. Do not claim a parent and child concurrently, drop prerequisites to acquire a claim, reuse an expired fence or use another target’s private token. The authenticated owner, token/fence, lease and revision are checked on the target; distinct targets can have independent claims for the same actor.

## A complete small fix

Suppose current source proves a search handler validates emptiness before trimming, while its maintained guide says whitespace-only input returns no rows. One packet can capture the defect, minimal change and meaningful test. This is a native MCP request example with synthetic nonzero IDs; use the Session ID really created for the work.

~~~json
{
  "name": "{{ mcp_tool_task_create }}",
  "arguments": {
    "selection": {"project_id": "00000000000000000000000002", "project_version": "main"},
    "input": {
      "idempotency_key": "trim-before-empty",
      "session_id": "00000000000000000000000001",
      "references": [{"target": {"type": "session", "id": "00000000000000000000000001"}, "relation": "derived_from"}],
      "title": "Skip repository lookup for whitespace-only searches",
      "description": "R1: whitespace-only input returns [] with zero repository calls. R2: nonempty input preserves normal search behavior. Current search validates before trimming. Trim once before testing emptiness; reuse the existing repository, caller authority and error path. Own only the handler and its regression. Run the focused empty/whitespace/nonempty cases and compare the maintained empty-query guide with actual results.",
      "task_type": "bug",
      "priority": 2,
      "checks": [
        {"kind": "acceptance", "text": "[A1] Whitespace-only requests must return [] without repository access; nonempty trimmed requests must retain existing behavior."},
        {"kind": "test", "text": "[T1] In the existing fixture, verify empty/spaces/tab/newline cause zero repository calls and ' abc ' reaches the repository once as 'abc'; compare the guide with the tested behavior."}
      ]
    }
  }
}
~~~

Creation returns `result.specification.id`. Claim that returned ID with a positive policy-accepted lease:

~~~json
{
  "name": "{{ mcp_tool_task_claim }}",
  "arguments": {
    "selection": {"project_id": "00000000000000000000000002", "project_version": "main"},
    "input": {"id": "00000000000000000000000003", "lease_ms": 600000}
  }
}
~~~

The duration is explicit example input, not a framework default. The claim's `result.record` and private `result.token` supply the actual fence. Use the record's `specification.revision`, not a guessed increment. No token value is illustrated or manufactured here.

## Reconcile discoveries before incompatible work

New questions trigger focused Task/Memory recall and selected AST/Knowledge reads; reuse sufficient evidence. Preserve completed specifications and check evidence as provenance; their optional commit association is the metadata exception described below. A newly discovered requirement or interface mismatch changes the affected specifications, checks, coverage and ordering before affected implementation.

Use `{{ mcp_tool_task_revise }}` with its actual input: `target`, `lease_ms`, `reason`, changed specification fields and typed `add_checks`. Omission preserves fields; null clears only fields whose current schema permits it. Do not send flat claim_token, add_acceptance_criteria, tests or dependency edits copied from another protocol. Use the advertised links mutation for parent/dependency changes. Read the returned gates/revision and revalidate affected checks against the new behavior.

If a required producer is absent, preserve the concrete contract, affected requirement and exact prerequisite in the current packet. Do not implement a mock authority, Boolean grant callback, compatibility wrapper or guessed default. Continue independent authorized work. Completed deliveries cannot be reopened by a fabricated operation; save genuine corrective work under the applicable still-open outcome when necessary and avoid dependency cycles.

## Evidence and completion

Every changed work unit compares source and maintained docs in both directions. Name the source/symbols, documented commands/contracts and actual observations; resolve drift or retain the unresolved condition. Integration review cannot postpone that obligation.

Record each active native check by the producer-returned canonical check ID, with concrete evidence and actual passed/failed status. Supersede obsolete checks with reason/replacement; do not mark them passed. A parser, fixture or in-process peer proves its recorded boundary, not an installed host, vendor/model delivery or whole product.

`{{ mcp_tool_task_progress }}` uses the current owned target plus lease_ms, summary and next_step. Preserve result/artifacts, evidence, decisions, failures/unrun work and exact continuation; refresh the revision from its result. A flag prevents premature completion; clear it only through a supported admitted operation after recording the resolution. Never replay an uncertain write automatically or claim rollback when the producer reports a committed/uncertain effect.

Complete only after actual checks, dependencies/descendants and documentation gates pass. Completion uses the operation's advertised `input.id` and `input.claim`; it does not accept a generic progress payload. When interrupted, release with current fence, summary and exact next step. The coordinator checkpoints cross-Task effects in the Session; closing a leaf does not close the demand.

## Associate the delivered commit

After an authorized commit or amend succeeds, run `git rev-parse HEAD` in the repository that contains the delivered changes. Update only the related Tasks through `{{ mcp_tool_task_commit_set }}` using `input={id, expected_revision, commit, reason, claim?}`. `commit` is the full lowercase 40- or 64-character Git object ID; null explicitly clears a wrong association. Read the current Task revision first. A live owner must supply its current claim fence, whose revision must agree with `expected_revision`; do not bypass another agent's claim. Unowned Tasks, including completed or cancelled records, use the authenticated Task write operation without acquiring a claim or reopening the record.

The update preserves status, checks, progress and existing references, and records actor, reason and previous associations in lifecycle history. Repeating the same association at the current revision is a no-op. After amend, replace the previous SHA only on Tasks included in that amended commit; do not rewrite unrelated or historical Task snapshots. A single commit may cover several Tasks. Without Git, leave the optional field empty.

If the Task update fails after Git succeeded, report the committed SHA and the pending update, read back current state and resolve the concrete failure through the supported API. Do not claim the commit rolled back, retry a stale revision blindly, fabricate delivery, or initiate push. This procedure does not grant permission for commit or push.
