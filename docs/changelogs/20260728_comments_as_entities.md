# Comments become entities in every tree-sitter language, with an edge to what they document

**Date:** 2026-07-28
**Scope:** `internal/ast/treesitter_adapter.go`, `internal/ast/helper.go`,
`internal/ast/comment_entity_test.go`, `internal/ast/docstring_pipeline_test.go`
**Origin:** Engineer's request — comments indexed as entity with `type` Comment and
`name` being the text, pointing to the associated entity or, as last resort, to the file

---

## What changed

PL/SQL already turned `COMMENT ON` into `Comment` entities whose name is the text, so
"what the documentation says" was searchable. In every other language a comment only
existed as a `Docstring` field hung on a declaration — which means **a comment that documents nothing was not indexed**: license header, note inside a function
body, explanatory block between functions, commented-out code.

Now each comment is a `Comment` entity with `Name` equal to the text itself, and carries
a `REFERENCES` edge:

- to the declaration it precedes, when there is one;
- to the file, otherwise — nothing is left unreachable for lack of owner.

## How, without reintroducing the traversal

Comments are not reachable via per-language query files: those describe
declarations, and no language declares a pattern for its own comments. Scanning the
tree for them would reintroduce exactly the whole-file traversal that was just
removed.

Instead, `commentQueryFor` **synthesizes a query** from the comment node types
the grammar actually has, and it runs in the same pass, via the same engine, on the C side. The query
is cached per grammar.

Missing grammar types are discarded instead of forwarded: **a single unknown
node type makes the entire query fail to compile**, and the set of comment types is
the union of all languages, so most of it doesn't exist in any one of them
individually. `IdForNodeKind` decides what remains.

## `cleanDocstring` only stripped prefix

It never stripped suffix, so a single-line Python docstring came out as
`Alpha docstring."""` and a single-line block comment kept `*/`. This was one of
two defects pinned in tests with the defect named, and it matters more than before:
**the Comment entity name is the text itself**, so the junk becomes visible to search.

Fixed: suffixes `"""`, `'''`, `*/`, `-->` are stripped, and `--` and `<!--` enter the prefix
list.

`TestDocstringsSurviveTheRealQueryPipeline` went red with this — it was the test that pinned
the defect. Expectation updated to the correct value and defect comment removed. It was
exactly the scenario anticipated when the defect was pinned: whoever fixes it sees red and needs
to know it's a fix, not a regression.

## Deduplication

Identical comments in the same file produce a single entity. This is the already
existing behavior for non-specific labels, and here it's desirable: separators like `// -----`
would generate hundreds of identical entities.

## The ANTLR side

Drivers build the stream with `antlr.NewCommonTokenStream(lexer, antlr.TokenDefaultChannel)`
and in those grammars comments go to the `HIDDEN` channel, so **they are not in the tree**
`Parse` returns. They aren't lost: `CommonTokenStream` buffers every token the lexer
produced and only filters by channel on access, so after parsing they can be read back.

- `antlrcommon.CollectComments` calls `Fill()` first — the parser may have stopped before EOF,
  due to parse error or because the entry rule ended, and trailing tokens would never have been
  pulled from the lexer — and returns comment tokens as `TreeNode`.
- Result is appended to the root in **`TreeNode.Comments`, not `Children`**. `Children` is what
  extraction patterns walk, and injecting nodes there would change what every existing pattern matches.
  A separate field crosses the sidecar JSON on its own.
- "Not on the default channel" would be too broad to mean "is comment": the hidden channel
  carries whitespace and, in some grammars, directives. Decision is by token name
  in the grammar — all five name theirs with `COMMENT` inside.
- All five native drivers got one line each.

### Ownership by proximity, not structure

The tree-sitter side decides structurally: the comment owns the declaration that is its next
sibling. There's no equivalent here, because comments were never in the tree to have
siblings. `extractCommentsAntlr` uses proximity: comment belongs to the first entity that
starts up to `commentAttachGap` (2) lines below it, and to the file when nothing is that close.
Runs last, because it needs line numbers of all already-known entities.

**Sidecar caveat:** grammars installed as a separate binary only produce comments
after being rebuilt with this change. Since the field is JSON, old binaries simply
omit it and result is `nil` — degrades silently, without error.

## Tests

`TestCommentsAreEntitiesInEveryLanguage` covers Go and Python with the three cases that matter:
header comment pointing to the file, declaration comment pointing to the
declaration, and note inside a function body pointing to the file.
`TestCommentNamesCarryNoMarkers` ensures no marker survives in the name.
`TestCommentsAreEntitiesInAntlrLanguages` walks the entire ANTLR route, from lexer channel
to indexed entity with edge, and verifies header goes to file while the block glued to the function goes to the function.

Full suite with `-race` clean.
