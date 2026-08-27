Reevaluate the bounded planner when upstream delivers a selective recursive join in Icebug

Upstream Support for Selective Recursive Join in Icebug

## Context

The planner introduced in commit fcaa9d7 (`internal/ast/ladybug_icebug_traversal.go`) intercepts only a narrow and semantically safe subset of reachability (`*1..N`, N<=8, returning only the endpoint reached, with selective anchor). The rest of the Cypher recursive traversal over Icebug graph continues falling into the engine's `READ_FTABLE → RECURSIVE_EXTEND` plan, which enumerates the entire node table before expanding and exceeds timeout (>30 s measured against ~10 ms native).

Upstream issues that govern this include:
- Recursive join backlog: https://github.com/kuzudb/kuzu/issues/4285
- Global initialization of structures: https://github.com/kuzudb/kuzu/issues/4941
- Explosion in 2 hops: https://github.com/kuzudb/kuzu/issues/4459
- Filter placement: https://github.com/kuzudb/kuzu/issues/5040

When there is upstream movement (or authorized go-ladybug upgrade beyond version 0.17.0):

1. Reassess the `EXPLAIN` of five cases in `TestIcebugRealGraphThreeHopPlans` (`internal/ladybugstore/icebug_realgraph_test.go`) with the new engine: if `RECURSIVE_EXTEND` passes after consuming the anchor's predicate before expansion, controls should fall close to the boundaries' times.
2. If the plan becomes selective, deliberately reduce the intercepted subset by the planner (the parser is narrow in purpose; less interception = fewer semantic divergence surface).
3. Remember: alternative relationships (`[:A|B]`) have known defects in the reader — the planner queries TYPE and TYPE_REVERSE separately for this reason; test to ensure the defect persists before making any changes.

## How to Know It Worked

- The recursive controls of `TestIcebugRealGraphThreeHopPlans` complete below seconds with GRAPHIT_REAL_STORE pointing to the real corpus.
- The set returned by the public query on 3 hops remains identical to the native storage, with the planner intercepting fewer forms than today.
