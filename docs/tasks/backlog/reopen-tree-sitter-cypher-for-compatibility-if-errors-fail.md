# Reopen tree-sitter-cypher in compatibility if fail-closed errors justify it

# tree-sitter-cypher as a conditional improvement to the compatibility layer

Current decision (Engineer, 2025-08-25): canonical compatibility uses a STRICT fail-closed
REGEX — it translates only exact forms (`[:TIPO]`→members when there is no filter; UNION per
member with a filter; `uid='lit'`→IN) and ERRORS actionably, listing the member tables instead of guessing.
No double fallback is kept.

## STATUS: REMOVED (2026-08-26)

Engineer's decision: tree-sitter-cypher does not make sense at the moment. The strict
fail-closed regex covers 100% of the current needs without a native external dependency.
The question can be re-evaluated if (and only if) the volume/nature of fail-closed
rejections justifies the cost of maintaining a native grammar.
