Accelerators Post-T18 (none block coredude; suggested order of value/cost)

Context:
T18 closed with canonical layout (node by label, rel by real pair, opt-out mirrors,
planner member-aware without hop ceiling, sanitizer uid=→IN). Permanent benchmarks: local ~182 ms / S3 ~694 ms for a 3-hop impact query; native ~10-16 ms.

Items, with each example resolving themself

1. **pre-computed component_id in export** (WCC on CALLS as column of node parquet).
   Resolve: "X reaches Y?" in O(1); the planner's BFS is restricted to the anchor component.
   The engine does NOT use the column alone (RECURSIVE_EXTEND does not push predicate — measured);
   consumption is by the planner + point-to-point connectivity queries.

2. **Sort by direction/members based on the statistics of manifest** (`Rows`, members already exported).
   **IMPLEMENTED**: Members are iterated from smallest to largest (line 1157 in internal/ast/ladybug_icebug_canonical.go). Resolution: Boundary explosion when anchoring outbound on a hub (Errorf has ~3.911 callers).


5. **Bidirectional in the planner** when both endpoints have selective predicates.
   Resolve: `WHERE c.name CONTAINS 'Handler' AND t.uid IN [...]` without exploding the
   side Handler. Implement as a bidirectional expansion of boundaries when both
   endpoints have selective predicates.

How to Know It Worked

Each item is implemented with its own benchmark against the numbers above and tested for equality with native code.
