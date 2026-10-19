---
references:
  - target: {type: task, id: tsk-b6cd}
    relation: implements
  - target: {type: task, id: tsk-6f63}
    relation: uses
  - target: {type: task, id: tsk-4e73}
    relation: uses
---

# Rust Task specification transitions

The Records library proposes revisions, check verification and supersession over immutable Task observations. The original SnapshotContext supplies cancellation and deadlines. The library does not read or mutate a TaskStore, authorize requests, manage leases, or publish history. Its consumer must enforce those operations using fresh authoritative state.

TaskSpecification contains the observed TaskId, positive revision, title, description, task_type, priority (0 through 4), checks and optional validated [schedule](task_schedule.md). TaskId and CheckId are distinct NonZeroU128 wrappers; they do not generate identifiers. Actor strings and audit timestamps are observed metadata, not authority or agenda.

The three planners take the observation, a typed command and the original SnapshotContext:

- plan_revision proposes changed specification metadata and optional new checks.
- plan_check_verification records pass/fail evidence for an active check.
- plan_check_supersession retires a check, preserving its evidence, and optionally adds a replacement.

Each returns Result<TaskSpecDelta, TaskSpecError>. before() borrows the complete unchanged observation, while after() and audit() retain owned data. into_parts() returns all three parts for the real consumer to preserve and publish. Expected revision must match; checked addition rejects revision overflow. Successful planning alone is not a storage commit.

## Revision semantics

FieldUpdate::Keep preserves a value. Set replaces it; title/description/type text is trimmed and must be nonblank. Equal normalized text does not count as a change. Missing serde fields use Keep with #[serde(default)]; present fields deserialize as Set. For schedule, Set(None) clears the agenda and Set(Some(schedule)) replaces the entire validated agenda. Equal agendas, Keep and clearing an already absent agenda are no-ops unless another field changes.

An effective title, description or task_type change resets every active check to Pending and removes its verification. Superseded records and their evidence remain unchanged. Priority and schedule changes preserve all existing check results. New logical checks start Pending; duplicates and blank additions are ignored, including duplicates of historical checks. A new logical check cannot reuse an existing ID.

All successful metadata changes advance the revision and append the proposed revision audit. Schedule is conserved by verification and supersession as well as ordinary revisions. No planner derives schedule from audit, creation, completion, lease timestamps or dependencies.

## Check invariants

Canonical observations use trimmed, nonempty text, unique IDs and unique (kind, text) check identities. At least one active Acceptance and one active Test check must remain. Pending checks have neither verification nor supersession; Passed/Failed checks have verification; Superseded checks retain supersession and may retain old verification. Historical replacements must reference distinct existing checks without cycles. Validation traverses those chains iteratively and reuses completed tails.

Verification requires an active known check and nonblank evidence/actor. Repeated verification is allowed. Supersession preserves prior verification, records actor/reason/time and optionally creates a new Pending check. Replacement kind defaults to the old kind; an explicit different kind is valid when both required active kinds remain. Unknown/inactive targets, logical/ID collisions and lost required kinds reject without changing the observation.

TaskSpecError distinguishes invalid input, no change, stale observation, unknown/inactive checks, duplicate IDs/replacements, missing required kinds, revision overflow and the original request error. Cancellation (Interrupted) and deadline expiry (TimedOut) preserve the original io::Error and take precedence over invalid input. No allocation or work-accounting framework is part of this API.

## Validation boundary

Focused tests cover complete before/after history, normalization, check resets and preservation, deduplication, replacements, revision errors, original cancellation/deadline and schedule propagation. Calendar/JSON tests exercise the real Chrono and Serde codecs. The public Rustdoc example compiles the current three-argument planner API.

This delivery qualifies the pure Records library. Native TaskStore persistence, MCP/CLI/HTTP DTOs and the React Gantt catalogue are separate producer/consumer deliveries; their operational behavior is not established by these domain tests.
