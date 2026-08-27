# Reabrir tree-sitter-cypher na compatibilidade se erros fail-closed justificarem

# tree-sitter-cypher como melhoria condicionada da camada de compatibilidade

Current Decision (Engineer, 2025-08-25): Canonical Compatibility Uses STRICT REGEX
fail-closed — traduz somente formas exatas (`[:TIPO]`→membros quando sem filtro; UNION por
Member with filter; INLINE 0 → IN and ERROR is triggered by listing member tables instead of jumping.
No double fallback is retained.

## STATUS: REMOVIDO (2026-08-26)

Engineer's Decision: Tree-sitter-Cypher doesn't make sense right now. The strict regex
failsafe closed circuit covers 100% of current needs without external native dependency.
The issue can be reconsidered, and only then, if (and only if) the volume/character of rejections
Justify the cost of maintaining a native grammar.
