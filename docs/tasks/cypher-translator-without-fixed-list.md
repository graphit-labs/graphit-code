# Task: Cypher translator rewrote label predicates backwards, and via a fixed list

**Status: done** on 2026-08-04. Continuation of `docs/tasks/schema-before-query.md` — the
next session reported another query failing, and this time the fault wasn't the skill.

## The symptom

```
MATCH (f:File)-[:CONTAINS]->(fn) WHERE ... AND (fn:Function OR fn:Struct OR fn:Method OR fn:Interface) ...
→ Parser exception: Invalid input <... AND (fn:`Function` OR>
```

What LadybugDB received had `fn:\`Function\``, `fn:\`Struct\`` and `fn:\`Interface\`` intact and
`label(fn) = 'Method'` rewritten — **one of four translated, three not**. That asymmetry
reveals the bug: it's not the agent writing it wrong, it's the translator choosing.

Reduced to the minimum, against the live graph:

| query | result |
|---|---|
| `MATCH (n) WHERE n:Method RETURN count(n)` | **743** |
| `MATCH (n) WHERE n:Function RETURN count(n)` | **Parser exception** |

## The cause: two passes in the wrong order

`translateLadybug`, in `internal/ast/ladybug.go`, did:

1. **escape** — for each label in a hardcoded `allLabels` map, `:Label` → ``:`Label` ``;
2. **label predicate rewrite** — `reLabelFilter` = `(?i)(WHERE\s+|AND\s+|OR\s+)(\w+):([a-zA-Z0-9_]+)`
   → `label(x) = 'Label'`, which exists precisely because the `n:Label` form in WHERE is Neo4j
   syntax that Ladybug's parser rejects.

But `[a-zA-Z0-9_]+` doesn't match backticks. After pass 1, pass 2 had nothing left to match —
**the rewrite only fired for labels that were MISSING from the escape list.** Inverted: the
more well-known the label, the more certain the crash. `Method` worked because it was missing.

A second, independent defect: the regex required `WHERE`/`AND`/`OR` glued to the variable, so
`AND (fn:Function` — with a parenthesis — never matched. The first alternative of any parenthesized
group was left behind even when the others were rewritten.

Third: the fixed list had **50 of the 114** labels declared by `graph_label:` in the grammars.
Missing were `Method`, `Heading`, `CodeBlock`, `Attribute` and 60 more.

## The decision: there is no list

The Engineer's question was the right one — *what is that allLabels for? is it necessary?*

It only served to decide **which words get backticks**, and backticks in label position are safe
for any identifier, reserved or not. In other words: **position** already identifies the label, the
name is irrelevant. And the list could never be correct — labels come from `graph_label:` in YAML
files loaded at runtime from three directories (runtime, user, project), so a user grammar
introduces a label that this binary has never seen, and a compiled list doesn't fail loudly when it
drifts: it simply stops escaping.

So the whole list was removed, along with the ~100 regexes it generated:

- `rePatternLabel` — escapes `:Label` when it is in pattern position: after `(`, `[` or `|`,
  with optional variable and spaces. `(n:X)`, `(:X)`, `-[r:X]->`, `(n : X)`.
- `reLabelPredicate` — rewrites the predicate, now **before** escaping, and with a prefix that
  consumes `NOT` and opening parentheses, so every alternative in a group is handled.

## What else didn't make sense in the same function

- **`strings.Contains(qUpper, "CREATE INDEX")` → `RETURN 1`.** The test was on the text, not on
  the statement: searching the codebase for the comment `'CREATE INDEX'` returned 1 instead of the
  comments. It became `reDDLNoop`, anchored at the start.
- **`strings.ReplaceAll(q, "labels(n)[0]", "label(n)")`.** Literal replace tied to the variable named
  `n` — `labels(f)[0]` slipped through. It became a regex with capture.
- **No pass respected string literals.** A uid is `internal/x/y.go::Apply.Apply` and a search
  term can be the text `(n:Function)`; rewrites entered inside quotes and changed what the query
  asked for, silently. Now everything goes through `mapOutsideStrings`.

## Tests — `internal/ast/ladybug_translate_test.go`

`translateLadybug` **had no tests at all**, which is the root cause of everything above. Nine now,
including the ones that matter: the full reported query; the predicate for known labels (which was the
inverted case); `NOT` and nested parentheses; positional escaping including invented labels;
preserved literals with uid and escaped quotes; real DDL becoming a no-op and DDL mentions not
becoming one; and the DDL that the indexer itself emits passing through intact.

The guard that replaces the list: `TestTranslateCoversEveryShippedGrammarLabel` reads every `graph_label:`
from the repo's YAMLs and requires that each one works in both positions. With positional escaping it
passes by construction — its value is to fail if someone reintroduces a list.

## The two debts, closed in sequence

**`CreateGraphSchema` in `internal/ast/schema.go` was dead three times.** (Previously noted as
`ensureIndexes`; this is the name.) It emitted, per label, one `CREATE CONSTRAINT` and three
`CREATE INDEX`, each with a fallback — eight statements. All became `RETURN 1` in the translator, and
the fallbacks never ran because the first call never errored. And the third layer: the **four**
calls — `runSyncPhase1`, `runASTWatch`, hub `Install`, `reindexAST` — passed zero labels to a
variadic function, so the loop didn't iterate even once. The function and the four calls were removed.
The engine has no secondary index or constraint, so `RETURN 1` remains correct; what was left was the
function.

**The `coalesce(` → `COALESCE(` rewrite was useless.** Checked against the database:
`RETURN Coalesce(NULL, 'ok'), CoAlEsCe(NULL, 'ok2')` returns `ok|ok2` — function names in
LadybugDB are case-insensitive, so the rewrite never had any effect, and moreover it only caught the
lowercase form. Removed.

With `CreateGraphSchema` gone, nothing in this code emits index DDL anymore, so `reDDLNoop` now
exists only for a hand-written query in the Neo4j dialect — as stated in its comment.

**The `-[r:A|B]->` alternation also closed.** In an alternation only the first alternative carries
a colon, so `rePatternLabel` escaped `A` and skipped `B` — which breaks if it collides with a
reserved word. A pipe alone doesn't identify an alternative: a list comprehension
(`[x IN xs | x]`) also has a pipe inside brackets, and rewriting there would corrupt the query.
Hence the new step only acts on brackets whose content **starts with an optional variable and a colon**,
which is the shape of a relationship pattern. Covered by `TestTranslateEscapesEveryRelationshipTypeAlternative`,
which includes the two list comprehensions as a negative case.
