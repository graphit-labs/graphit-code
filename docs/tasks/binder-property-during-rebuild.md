Task: The inline 0 was accusing the query when the guilty party was the rebuild

Status: completed on August 5, 2026. The series began at `docs/tasks/schema-antes-da-query.md`.

## O sintoma

```
MATCH (f:File)-[:CONTAINS]->(e) WHERE f.path IN ['internal/ast/antlr_adapter.go', ...]
  AND label(e) IN ['Function','Method','Struct']
  RETURN f.path, label(e) AS type, e.name, e.line_number, e.end_line ORDER BY f.path, e.line_number
→ Binder exception: Cannot find property line_number for e
```

## A query estava certa

My first hypothesis was modeling: _`e`_ without label, the group `CONTAINS` includes
`FROM Directory TO File`, `File` does not have `line_number`, so the binder would refuse. I tested against a complete graph built in a temporary directory and all six forms connect:

---

Note: The "Inline" sections are placeholders for code blocks or technical details that were omitted from this translation.

```
MATCH (n) RETURN n.line_number                                  OK
MATCH (n) RETURN n.is_exported                                  OK
MATCH (f:File)-[:CONTAINS]->(e) RETURN e.line_number            OK
MATCH (f:File)-[:CONTAINS]->(e) WHERE label(e) IN [...]         OK
MATCH (f:File)-[:CONTAINS]->(e:Function) RETURN e.line_number   OK
```

The LadybugDB rule is "some table candidate has the property," not "all have." What
overturned the query was the **graph state**: INLINE_7 was in flight, and at that moment the database had only INLINE_8, INLINE_9, INLINE_10, and INLINE_11 — the latter two in reduced form (INLINE_12), without INLINE_13. None of the tables had the property, so the binder rejected a correct query.

Conferido no mesmo momento: `MATCH (f:File) RETURN count(f)` devolvia **2**.

This makes the error worse than a failure.

A transparent admission calls for fixing what is wrong. This call for fixing was made when she had me as a client before I started to test. It's noted that the message is **identical** to an invented name (`n.line`, `n.type`), and the agent cannot distinguish between them.

What was done

---INLINE_17--- passes to treat the two families of errors from the binder. For ---INLINE_19--- , the distinction is made by evidence: which tables in this graph carry the property.

- **No load** → The name doesn't exist, or the schema is partial. The message says both and shows the tables present because a "punch where there should be thirty" is the signature of a rebuild:
  > in this graph, a property named "line_number" exists on some labels but not all — it's on File. Pin the label in the pattern (`MATCH (n:Function)`), because `WHERE label(n) IN [...]` filters after binding and doesn't help here

- **Some loads** → The label pin is wrong, and the message names who has:
  > "relative_path" exists but not on every label this pattern can match — it's on File. Pin the label in the pattern (`MATCH (n:Function)`), because `WHERE label(n) IN [...]` filters after binding and doesn't help here

The skill claimed the opposite of the engine in two places, and this is the root cause for me to have started with the wrong hypothesis:

- "You may ONLY access properties shared by ALL labels: name, path, line_number, end_line,
  docstring, lang" — false in both directions. `is_exported` and `cyclomatic_complexity` match without a label; the list is safe for properties that `File`/`Directory` do not have.
- "Narrow with `label(n) IN [...]` before touching the property" — does not work: binding happens before filtering. The label must be in the pattern, and multiple labels mean multiple `MATCH` combined by `UNION ALL`.

Note: Inline codes and markdown are preserved as is.

The recovery protocol for the ____INLINE_32__ has moved from two causes to three. The third one is marked as the cause that makes a correct query appear incorrect.

## Testes

The two states are combined into the same test:
partial schema (only `initSchema`) rejects `line_number` with the message "rebuild"; after that query is executed, it connects — this proves that the message was about the schema, not about the query. And `n.line`, which does not exist in any state, continues to fail by offering the real name.

The inline 38 covers the other branch with inline 39, which only exists in inline 40.

The inline 41 has won the inverse: fail if skill says that a match without label only reaches what all labels share. **This test fixed the wrong assertion** — it was he who failed to correct the text when I corrected it, which is the right behavior for a content test and worth noting: a document test also needs to be checked against the system, not just against the previous text.
