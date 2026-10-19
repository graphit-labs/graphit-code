# Functional Rust record gates

Main `graphit_records::gates` implements Task readiness and closure over immutable borrowed facts. Persistence and authorization belong to the Task service.

The standard-library-only kernel borrows immutable Task facts and evaluates readiness and closure. It performs no I/O, mutations, claim operations, fencing, authorization, reference or audit writes. GateGraph is a conditional evaluation, never authority or proof of complete capture. The authoritative caller must select all relevant facts and revalidate original cancellation/deadline, Auth project/version, current revision and owner/token/epoch/lease in the same atomic publication boundary. A read must not repair missing state.

## API and ownership

TaskStatus has Open, InProgress, Completed and Cancelled. CheckKind has Acceptance and Test; CheckStatus has Pending, Passed, Failed and Superseded. CheckFact borrows evidence. TaskFact borrows id, session_id, parent_id, dependency IDs and checks, and carries status and flagged.

- GateGraph::new(&[TaskFact]) -> Result<GateGraph, GateError>
- ready(&self, id: &str) -> Result<bool, GateError>
- completion(&self, id: &str) -> Result<bool, GateError>
- session_closable(&self, session_id: &str) -> Result<bool, GateError>

Queries use shared borrowing and cannot mutate facts. Rust's lifetime checks prevent returning a graph beyond the lifetime of its facts; the module includes a compile-fail example for that boundary.

```rust
use graphit_records::gates::{CheckFact, CheckKind, CheckStatus, GateGraph, TaskFact, TaskStatus};
let checks = [
    CheckFact { kind: CheckKind::Acceptance, status: CheckStatus::Passed, evidence: "observed acceptance" },
    CheckFact { kind: CheckKind::Test, status: CheckStatus::Passed, evidence: "executed fixture" },
];
let facts = [TaskFact { id: "task", session_id: "session", parent_id: None,
    status: TaskStatus::Open, flagged: false, dependencies: &[], checks: &checks }];
let graph = GateGraph::new(&facts)?;
assert!(graph.ready("task")?);
assert!(graph.completion("task")?);
assert!(!graph.session_closable("session")?);
# Ok::<(), graphit_records::gates::GateError>(())
```

## Validity and predicates

All fact and query IDs must be nonempty and equal to their trimmed form. Reject surrounding whitespace instead of normalizing it. Construction rejects duplicate Task IDs, missing dependency/parent targets, duplicate dependencies within a Task, self edges and parent edges between sessions, including on Cancelled facts. Dependencies between sessions are allowed. Ordering edges run dependency→dependent and child→parent. A dependency contributes an ordering edge only when neither endpoint is Cancelled; a child also listed as a dependency contributes two legal parallel edges when neither endpoint is Cancelled. Cycles in this remaining combined ordering graph remain invalid; hierarchy cycles remain invalid in every status.

An active check is not Superseded. A Task meets its own closure conditions when unflagged, with at least one active Acceptance and Test check, every active check Passed, and nonblank evidence for every active check. Superseded checks neither supply a required kind nor block completion.

Completion conditions combine those own conditions with every dependency validly completed and every child resolved. A child is resolved when validly completed, or Cancelled with all of its descendants also resolved. Cancelled work is withdrawn from the parent scope; its own checks and dependencies do not certify delivery. An Open/InProgress or invalidly completed descendant below a Cancelled child still blocks the parent. Valid completion requires status Completed and those conditions transitively. A historical Completed status with invalid checks/flags/ancestors does not release downstream work.

Dependencies belonging to a Cancelled task remain recorded and validated, but impose no prerequisite ordering or satisfaction requirement. For example, a cancelled child depending on its parent must not create an ordering cycle that prevents evaluating that parent. Hierarchy evaluation still waits for the child's descendants. Conversely, an active task depending on a Cancelled task has a known false prerequisite, without an ordering edge: cancellation never satisfies a dependency, even when that cancelled task's descendants are all resolved. An unfinished grandchild depending on its cancelled parent therefore remains unfinished and blocks the ancestor through the hierarchy.

| Query | Conditional result |
|---|---|
| ready | Open and every dependency validly complete; own flag/checks or unfinished children do not block readiness. |
| completion | Conditions above, for Open/InProgress/Completed; always false for Cancelled. A true value does not authorize mutation. |
| session_closable | Every matching Task Cancelled or validly completed. Cancellation does not certify delivery or satisfy a dependency; a cancelled child may resolve withdrawn scope under the rule above. |
| Nonempty session with no matching facts | True over supplied facts; does not prove existence or complete capture. |

Cancelled scope/reasons remain the caller's durable contract. Dependency and child edges are evaluated separately, so a cancelled child also named as an explicit dependency still blocks the parent. Queries never cascade changes. Existing product procedures are documented in docs/specs/task_module.md and docs/specs/task_sessions.md; this module implements the conditional kernel.

## Implementation and failures

A standard HashMap indexes borrowed IDs. Construction scans identifiers/checks, builds temporary adjacency, indegree and resolved-child vectors, and uses iterative Kahn traversal. It retains borrowed facts, the index and two result vectors; temporary adjacency is dropped after evaluation. Expected construction work is O(N + E + B), including hashed identifier/check text B; standard hash operations have no worst-case constant-time guarantee. Retained storage is O(N) slots plus borrowed facts, with O(N + E) temporary graph slots. No whole Dataset or tree capture is performed by this kernel.

InvalidFacts means an invalid identifier, relationship or cycle in the hierarchy/remaining prerequisite ordering. MissingTask means a valid queried ID is absent from these facts, not proof of access or global nonexistence. Errors contain no identifiers, bodies, evidence or tokens. Standard allocation behavior applies; no custom fallible-allocation contract or retry is introduced.

## Verification

The affected tests cover four Task states, transitive invalid completion, parent/child conditions, active/superseded checks, evidence and identifier whitespace, cancellation and withdrawn child scope, cancelled-child dependencies on ancestors, nested active descendants depending on cancelled parents, dual child/dependency edges, conditional empty-session closure, diamonds/parallel edges, active combined cycles, duplicates/missing/self relationships (including cancelled tasks), cross-session dependencies and a thousand-node iterative chain. Readiness queries use shared borrowing. Public and compile-fail examples verify the API and borrowed-facts lifetime.

Run from the project root:

```text
cargo test --locked --offline -p graphit-records --test gates
cargo test --locked --offline -p graphit-records --doc gates
cargo clippy --locked --offline -p graphit-records --all-targets -- -D warnings
```

These tests qualify the pure graph decisions. Persistent Task operations need separate real storage, concurrency, recovery and authorization tests.
