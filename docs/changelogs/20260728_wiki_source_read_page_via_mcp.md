# `wiki_source`: read wiki page via MCP, with the same slicing as source

**Date:** 2026-07-28
**Scope:** `internal/textslice/` (new), `internal/wiki/source.go` (new),
`internal/ast/source_service.go`, `internal/mcpstdio/tools_wiki.go`,
`cmd/graphit/commands/{wiki.go,runners.go}`, `internal/{knowledge,memory}/rule.go`
**Origin:** Engineer's request — the agent is often confined to its own workspace

---

## The problem

Skills say, in several places: *"search the wiki, **read the entity page**, follow the [[wikilinks]]"*. All wiki tools take `project_dir` and read on the agent's behalf —
`wiki_search`, `wiki_browse`, `wiki_log`, `wiki_xrefs`, `knowledge_search`, `memory_search`.

**Reading the page was the only step without a tool.** Only direct file reading remained. And it's
precisely the step that fails when the agent is confined to its own workspace: the page it
needs belongs to another project in the ecosystem, at a path it cannot open.

In other words: step 4 told to explore the sibling via MCP, and midway the instruction fell back to
a native tool that doesn't reach there.

## `wiki_source`

MCP and CLI, with **the same slicing options as `ast_source`**: `head`, `tail`,
`start_line`/`end_line`, `line_numbers`, and `pattern` with `regex`, `before` and `after`.

```
wiki_source(project_dir: "/path/to/project", path: "auth-flow")
wiki_source(project_dir: "/path/to/project", path: "auth-flow", head: 40)
wiki_source(project_dir: "/path/to/project", path: "auth-flow", pattern: "refresh token", before: 2, after: 4)
wiki_source(project_dir: "/path/to/project", path: "<slug>", wiki: "memory")
wiki_source(project_dir: "<sibling dir>", path: "<slug>")          # the motivating case
```

```bash
graphit wiki source auth-flow --head 40
graphit wiki source auth-flow --pattern token --before 2 --after 4
graphit wiki source correction-x --wiki memory
graphit wiki source auth-flow --project /path/to/other-project
```

### `path` accepts what other tools return

`wiki_search`, `wiki_browse` and `wiki_xrefs` return `Slug`; `knowledge_search` returns `Path`,
which is `Slug + ".md"`. So `path` accepts both forms, plus a path relative to the wiki directory — and matches
**case-insensitively**, because wiki filename is generated from title
(`1._Clean_Code.md`) and the slug the agent has in hand rarely matches exactly. Requiring exact
match would make the tool useless precisely with its own search results.

Got the slug wrong? The error **lists pages that exist**. That's the answer, not a reason to go back to
reading a file.

### Two error classes, not one

`ErrPageNotFound` distinguishes a wrong slug from a refused reference. Wrong slug is resolved by listing alternatives; a reference that **escapes the wiki directory** needs to keep its own reason — if both fell in the same place, the listing would bury the refusal reason.

Verified: `../../../etc/passwd`, `..`, `../outside.md` and absolute path are all refused, and
none is reported as "not found".

## DRY: slicing moved to `internal/textslice`

Everything in `ast.SourceService.GetSource` after fetching the text is pure text manipulation. Making
a second copy for the wiki would duplicate ~100 lines that would diverge on the first fix —
and the improvements skill itself calls that WET code.

The new package carries what's identical in both: `Apply` (range → head → tail → pattern, in that
fixed order), `Search` (matching, merged context windows), `FormatMatches` and
`FormatWithLineNumbers`. **Only text fetching differs, and only it stayed with callers** — code graph
on one side, wiki directory on the other.

`ast.SourceService` delegates the three leaf helpers. The entity-window arithmetic, which is the subtle and AST-specific part, stayed where it was — extracting that too would be risk without gain.

Side effect: the hand-written insertion sort that ordered indexes left, and with it
`TestSortInts`. `sort.Ints` does the same and needs no test here.

## The skills

Step **1b** entered the knowledge search sequence, between "search" and "read frontmatter",
with both reasons attached:

- **Takes project as a parameter.** File reading doesn't leave your sandbox; this tool
  reads on your behalf, so a sibling's page is the same call with another `project_dir`.
- **Slices.** Long page costs only the part you asked for, not all of it.

And the "when NOT to use the wiki" table gained an explicit note: **reading a wiki page is not on that
list.** Native file tools enter to *write* documentation, never to
retrieve it.

In the memory skill, recovery step 3 stopped saying "read the content" and now shows
`wiki_source` with `wiki: "memory"` — including the `pattern` form for long memory and the
`project_dir` form for a sibling. Step 4 (*"never read .md memory directly"*) now says **what
to do instead**, which is what was missing for the prohibition to be actionable.

`wiki_source` entered the mandate inventories for knowledge and memory.

## Tests

`internal/textslice`: composition order (head applies to selected window, not file),
absolute numbering preserved on tail, range past end is clamped instead of panicking, overlapping context windows merge without repeating lines, separator `---` only where a line was omitted, literal pattern matches case-insensitively and regex is not folded silently, invalid regex is reported.

`internal/wiki`: slug resolved in four spellings, slicing equal to source, directory-escape refusal in four forms, `ErrPageNotFound` distinct from refusal, empty entry rejected,
`ListPages` without extension, `firstHeading` skipping frontmatter.

One case in the merge test had the wrong expectation — I counted 6 lines where line 5
is legitimately omitted. Code was correct; became two tests, one for "there is a gap, has
separator" and another for "windows join, no separator".

Verified against this project's real wiki: `--head 12 --line-numbers` returns frontmatter
numbered, `--pattern DRY --before 1 --after 2` returns two groups separated by `---` with hits
marked, and `1._clean_code.md` matches `1._Clean_Code.md`.

`golangci-lint` clean.

> **Note about tree state:** another session was working on ANTLR/Oracle in this same
> tree during this step. None of it was committed nor reverted — this commit stages only the
> files for this step, by explicit path.
