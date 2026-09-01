---
title: OKF compliance audit (generator, consumers, UI) and title-first search results
status: done
created: 2026-08-29
updated: 2026-08-29
tags: [wiki, knowledge, memory, okf, ui, mcp, search]
---

# OKF compliance audit (generator, consumers, UI) and title-first search results

## Objective

Two things the Engineer asked for in the same request:

1. **Audit and finish the OKF refactor.** `docs/tasks/2026-08-20-okf-wiki-compliance.md`
   marked six tasks done and closed. It did not finish: the generator emits fields that are
   not OKF, three of our own consumers still read the pre-OKF shape, and the UI wiki explorer
   was only half-adapted. The claim "100% OKF" is not true today and the audit below says
   exactly where.
2. **Make wiki/knowledge/memory search token-frugal.** A search result should hand the agent
   a *title* and let the agent decide whether to spend tokens opening the source. The rule
   that generates the skills and the AGENTS.md mandate must say that explicitly, for both
   knowledge and memory.

### Reasoning — how the audit was done

The authoritative spec was NOT in the Hub (`hub_search "open knowledge format"` → 0 results),
so it was fetched from the source of record and read in full:
`https://github.com/GoogleCloudPlatform/knowledge-catalog/tree/main/okf` (`SPEC.md`, v0.2,
1006 lines). Every finding below cites the section of that spec it comes from. The runtime
evidence comes from `graphit_knowledge_lint` on this project's own wiki (242 pages).

## Audit — measured state on 2026-08-29

`graphit_knowledge_lint` over this project's knowledge wiki, 242 pages:

| Metric | Value | What it actually means |
|---|---|---|
| `errors` | 834 | — |
| `stalepages` | 240 / 242 | **Every** concept page. The linter reads `updated:`, which the OKF generator stopped emitting. Nothing is stale. |
| `missingfrontmatter` | 241 | 240 of them are `missingfields:[updated]`. Same cause. |
| `brokenlinks` | 354 | Mostly relative provenance links to files under `docs/` and body links to repository files, which `ResolveSlug` turns into phantom slugs like `..-..-internal-ast-pipeline.go`. |

None of those 834 errors describe a real defect in the wiki. They describe consumers that
were never updated when the producer changed.

### A. Generator does not match OKF v0.2

| # | Finding | Spec |
|---|---|---|
| A1 | Pages emit a flat YAML key literally named `generated.at:`. OKF has no such key: `generated` is a **mapping** `{ by, at }` and `generated.by` is REQUIRED inside it. `generated.at` is the spec's *prose* path notation, transcribed as a literal key. | §5.2 |
| A2 | `sources:` is a list of bare strings. Each entry must be a mapping whose `resource` is REQUIRED. | §5.1 |
| A3 | `index.md` carries a full frontmatter block. Index files contain **no** frontmatter; the single exception is a bundle-root `index.md` that MAY carry `okf_version`. | §8 |
| A4 | `okf_version: "0.2"` is never declared anywhere. | §12 |
| A5 | `log.md` has frontmatter and uses `## [2026-08-21 15:04:05] sync \| Compiled N changes` headings. §9 requires date-grouped `## YYYY-MM-DD` headings, newest first, entries as `* **Update**: …`. | §9 |
| A6 | **The memory index page was never converted at all.** `memoryIndexPage()` still emits legacy double-bracket links for every entry and pre-OKF frontmatter (`title`/`updated`/inline `tags: [..]`, no `type`). T2 of the prior task only touched `memoryEntityPageWithHash`. | §4.1, §6.1 |
| A7 | `appendMemLog()` is pre-OKF for the same reason. | §9 |
| A8 | Staleness is expressed as producer-specific `stale_since`/`stale_reason`; the lifecycle family defines `stale_after` (an absolute instant). Extensions are legal, but the specified field should be present when the concept is stale. | §5.5 |
| A9 | Cosmetic but misleading: `AutoLinkContent`'s doc comment still describes the legacy double-bracket form, while it emits standard Markdown links. `bm25PreFilter` and `multi_search` still use the legacy terminology in the prompts they build for the model. | — |

### B. Our own consumers still read the pre-OKF shape

| # | Finding | Effect |
|---|---|---|
| B1 | `internal/wiki/lint.go`: `requiredFields = {title, tags, updated}`. `type` — the ONLY field OKF requires — is not checked, and `updated` no longer exists. | 241 false "missing frontmatter" |
| B2 | `internal/wiki/lint.go`: `isStale` reads `^updated:`; absent ⇒ stale. | 240 false "stale" |
| B3 | `internal/wiki/lint.go`: `reFMField = ^(\w+):` cannot match `generated.at` — a dot is not `\w`. The field is invisible to the frontmatter scanner. | latent |
| B4 | `crossref.go` `BuildCrossRefGraph` and `FindWikiLinks` treat **every** relative markdown link as a wiki cross-reference, excluding only `http`/`https`/`file`/`#`. The Provenance line on every page and body links to repo files become phantom edges. | 354 false broken links, polluted xref graph, wrong backlinks |
| B5 | `internal/uiserver/wiki_handler.go`: `reFMTags = ^tags:\s*\[…\]` only matches the inline array form. OKF pages emit a block sequence. | tags always empty in the UI |
| B6 | `internal/uiserver/wiki_handler.go`: `reFMSource = ^source:` — pages emit `sources:`. | source always empty in the UI |
| B7 | Same file: page `type` is derived from the filename, never from the frontmatter `type:` that OKF makes mandatory. | the one required field is unused |

### C. UI wiki explorer

| # | Finding | Effect |
|---|---|---|
| C1 | `WikiMarkdown` treats **any** non-external href as a wiki page: `if (href.startsWith('wiki://') \|\| !isExternal)`. That captures `#anchors` and the relative repo paths in the Provenance line. | in-page anchors navigate to a non-existent page; provenance links dead-end |
| C2 | `WikiExplorerPage.preprocessContent` strips only the legacy double-bracket preamble. OKF cross-references use standard Markdown links. | the Cross-References list leaks into the rendered body, duplicating the sidebar |

### D. Search is not token-frugal

Measured today: `graphit_knowledge_search` / `graphit_memory_search` go through
`toonResult([]BM25Result)` — the *generic* TOON encoder, which applies no truncation of its
own. The bound comes from further upstream (`truncateSummary` at 200 runes, `snippetAround`
at 320) and `lancestore.DefaultLimit = 20`. So a default call costs roughly 20 × 200–320
characters of preview. It does not return "everything", but the preview is the bulk of the
payload and the agent did not ask for it.

The Engineer's rule: **a search returns the title; the agent decides whether to spend tokens
opening the source.** `graphit_wiki_source` already slices, so the second call is cheap and
targeted — which is exactly why the first call should not pre-pay for a preview.

## Plan & Task Breakdown

- [x] **T1 — Fix the consumers that read the pre-OKF shape (`internal/wiki/lint.go`)** —
  Spec: `requiredFields` becomes OKF's contract (`type`); `isStale` reads `generated.at`
  from the `generated` mapping with a legacy `updated:` fallback; the frontmatter field
  regex tolerates dotted keys. Done when `graphit_knowledge_lint` on this project reports
  0 stale and 0 missing-frontmatter for pages the generator wrote.
- [x] **T2 — Stop treating repo paths as wiki links (`internal/wiki/crossref.go`)** —
  Spec: a markdown link is a wiki cross-reference only when its target is a flat page slug
  in the same bundle directory — no `/`, no `./`/`../`, no non-`.md` extension. Applies to
  both `BuildCrossRefGraph` and `FindWikiLinks`. Constraint: legacy double-bracket link parsing
  must keep working. Done when broken links drop from 354 to the genuinely-broken remainder.
- [x] **T3 — UI server reads OKF frontmatter (`internal/uiserver/wiki_handler.go`)** —
  Spec: parse block-sequence `tags:`, read `sources:`/`- resource:` for the source field,
  and prefer the frontmatter `type:` over the filename heuristic. Constraint: the inline
  `tags: [a, b]` form must keep working for legacy pages.
- [x] **T4 — UI renders OKF links correctly (`WikiMarkdown.tsx`, `WikiExplorerPage.tsx`)** —
  Spec: only a bare same-directory slug becomes a wiki navigation button; `#anchor` stays an
  anchor; a relative repo path renders as an inert link. The preamble stripper recognises
  standard Markdown cross-reference lines.
- [x] **T5 — Generator emits conformant OKF (`internal/knowledge/wiki.go`, `internal/memory/wiki.go`)** —
  Spec: `generated: { by, at }` with an actor per §7; `sources:` entries as `- resource: …`;
  `stale_after` alongside the existing extension fields; `index.md` without frontmatter
  except `okf_version` at the bundle root; `log.md` in §9 date-grouped form; the memory
  index and memory log converted (A6, A7).
- [x] **T6 — Title-first search results (`internal/wiki/toon.go`, `tools_knowledge.go`, `tools_memory.go`, `tools_wiki.go`)** —
  Spec: the default result row is identity + ranking only (slug, title, type, score); the
  preview becomes opt-in. Constraint: `ai_optimized: false` (JSON) keeps the full struct, so
  nothing that consumes the JSON shape breaks.
- [x] **T7 — Rule and mandate say it out loud (`internal/knowledge/rule.go`, `internal/memory/rule.go`)** —
  Spec: both skills and both mandate triggers state that a search answers with titles and
  that reading a page is a separate, deliberate `graphit_wiki_source` call.

## Progress Log

### 2026-08-29
- Audit completed against OKF v0.2 `SPEC.md` fetched from `GoogleCloudPlatform/knowledge-catalog`
  (the Hub has no OKF knowledge artifact — `hub_search` returned 0 results; worth submitting one).
- Runtime evidence captured from `graphit_knowledge_lint` (242 pages, 834 errors).
- Task log opened with the plan above. Implementation starts at T1.
- T1–T7 landed. `go build ./...` and `go test ./...` clean; `tsc --noEmit` clean.
- **Correction received mid-task**: the Engineer said backward compatibility is not needed
  ("estamos em dev"). The legacy readers added in T1 and T3 — `updated:`, the dotted
  `generated.at:`, inline `tags: [a, b]`, singular `source:` — were REMOVED rather than kept
  alongside the OKF readers, and the tests that pinned them were rewritten to pin the OKF
  shape. Legacy double-bracket link parsing was kept, because it is an input format for
  hand-written source documents rather than a compatibility shim for our own artifacts.
- Discovered while verifying: the frontmatter was not merely non-conformant in its field
  names, it did not PARSE. Folded-scalar descriptions and colons in titles broke the block
  entirely. `YAMLScalar` was added and the conformance test now parses every generated page
  with `gopkg.in/yaml.v3` — 279 pages, 0 failures.
- `make install` run, so the CLI and daemon carry the change. **The MCP server backing this
  session was started before the install and still runs the previous binary**, so its search
  output and its lint numbers reflect the old code until the session restarts. The numbers
  reported above were measured by regenerating the wiki from the real docs tree inside a
  test, with the new code.

## Implementation Details

### New shared code

| File | What it is |
|---|---|
| `internal/wiki/okf.go` | `OKFVersion`, `OKFActor`, `WriteOKFGenerated`, `WriteOKFSources`, `YAMLScalar`. One place that knows what OKF requires, used by both generators. |
| `internal/wiki/okf_log.go` | `AppendOKFLogEntries` — the §9 log writer, shared by knowledge and memory. |

### `generated.by` is `process:<brand>-<module>-wiki`, not `<producer>/<version>`

§7 offers three actor forms. `process:` is the honest one — a wiki page is written by the
indexing daemon with no human in the loop — and it is also the stable one. Under
`<producer>/<version>` every release would change the `generated.by` line of every page in
every wiki, and pages are written only when their bytes change (`writePageIfChanged`), so
that is a full rewrite of the corpus per upgrade for a field nobody queries by version.
§5.3 also derives the trust tier from the `human:` prefix, so a generated page must not
claim one.

### `generated.at` keeps date granularity

ISO 8601 permits a date without a time. The stamp comes from the source's mtime so that
regenerating an unchanged wiki produces byte-identical pages; padding it to midnight would
assert a precision the input does not carry.

### Frontmatter is now quoted (`YAMLScalar`)

This turned out to be a conformance bug, not a nicety. §11 criterion 1 requires the
frontmatter to PARSE, and these blocks are assembled by string concatenation rather than by
a YAML encoder. A description lifted from a source document's own folded scalar arrived as
`description: > Where every artifact lives` — a block scalar header with trailing content,
which is a parse error that takes the whole block with it, `type` included. A title
containing `: ` broke it identically. `YAMLScalar` quotes anything that could be read as
YAML syntax and leaves unambiguous values bare, so the block stays readable.

### Which markdown link is a cross-reference

`isBundlePageLink` (in `crossref.go`) decides. The rule is the wiki's own shape rather than
a blocklist of URL schemes: a compiled wiki is FLAT — one directory of `<slug>.md` — so a
target carrying a path separator, a directory hop, or a file-shaped extension other than
`.md` cannot be a page in it. It is applied to legacy double-bracket links as well, because source
documents under the docs tree contain hand-written references to relative architecture pages
and a body is copied into the page verbatim.

### No compatibility with the pre-OKF shape

At the Engineer's instruction (2026-08-29, mid-task: *"não precisa de retrocompatibilidade,
estamos em dev"*), the readers do NOT accept the old shapes. `lint.go` reads `generated`
only — not `generated.at`, not `updated`. `wiki_handler.go` reads block-sequence `tags` and
`sources[].resource` only — not `tags: [a, b]`, not a singular `source:`. The wiki is a
compiled artifact regenerated from its sources, so there is no old page to stay compatible
with, and a fallback that is never exercised is a second definition of the format.

Legacy double-bracket link parsing stays, and that is not an exception to the above: it is an
INPUT format. Documents under the docs tree are hand-written, some contain wikilinks, and
`ResolveWikiLinksInBody` turns them into OKF links on the way in. Nothing GENERATES a
wikilink any more.

## Measured result

`LintWiki` over a wiki regenerated from this project's own docs tree (279 documents):

| Metric | Before | After |
|---|---|---|
| `errors` | 834 | 89 |
| stale pages | 240 / 242 | 39 / 281 |
| missing frontmatter | 241 | 0 |
| broken links | 354 | 50 |

The 50 remaining broken links are **true**: documents linking to `AGENTS.md`, `README.md`,
`LICENSE`, `SECURITY.md` — repository root files that are not wiki pages — plus illustrative page-name
placeholders inside the framework's own rule documentation. A flat `AGENTS.md` link is
structurally indistinguishable from a page slug, so the only way to tell is that the page
does not exist, which is exactly what the lint now reports. The 39 stale pages are documents
whose source mtime really is older than 30 days.

OKF §11 conformance, checked by parsing every generated page with `gopkg.in/yaml.v3`:
**279 concept pages, 0 unparseable frontmatter, 0 missing `type`.**

## Test Cases & Acceptance Criteria

### Feature: OKF conformance of generated bundles
Ref: A1–A8

#### Scenario: every generated concept page satisfies §11
```gherkin
Given a docs tree containing a document whose title contains ": " and whose description is a folded scalar
When the knowledge wiki is generated
Then every non-reserved page has a frontmatter block that parses as YAML
  And every such block carries a non-empty "type"
  And "generated" is a mapping carrying a non-empty "by" that is not a human: actor
  And every "sources" entry is a mapping carrying a non-empty "resource"
  And no page carries the flat key "generated.at"
```
Implemented by `TestGeneratedWikiConformsToOKF` (knowledge) and
`TestGeneratedMemoryWikiConformsToOKF` (memory).

#### Scenario: index.md and log.md follow the reserved-file structure
```gherkin
Given a generated wiki bundle
When index.md is read
Then its frontmatter contains only "okf_version" and that value is "0.2"
When log.md is read
Then it carries no frontmatter
  And its entries are grouped under "## YYYY-MM-DD" headings
  And each entry leads with a bold kind word
```
Implemented by `assertIndexFile`, `assertMemoryIndex`, `assertMemoryLog`,
`TestAppendOKFLogEntriesGroupsByDate`.

### Feature: a markdown link is not automatically a cross-reference
Ref: B4

#### Scenario: a link to a repository file is not a broken cross-reference
```gherkin
Given a document whose body links to "../internal/ast/pipeline.go"
When the wiki is generated and the cross-reference graph is built
Then that link produces no outbound edge
  And the lint reports no broken link for it
```
Implemented by `TestGeneratedPagesDoNotCrossReferenceRepositoryPaths`,
`TestBuildCrossRefGraphIgnoresProvenanceLinks`, `TestIsBundlePageLinkRejectsPathsAndFragments`.

### Feature: search answers with titles
Ref: D

#### Scenario: a default search hit carries no page text
```gherkin
Given a wiki hit whose summary is 600 characters long
When the result is formatted for an agent with no preview requested
Then the row contains the slug, the title, the type and the score
  And it contains none of the summary text
When the caller passes preview: true
Then the row additionally contains a bounded excerpt shorter than the summary
```
Implemented by `TestFormatSearchResultsTOONIsTitlesByDefault` and
`TestFormatBM25ResultsTOONIsTitlesByDefault`.

### Feature: the UI explorer reads the OKF shape
Ref: B5, B6, B7

#### Scenario: block-sequence tags, sources[].resource, and the frontmatter type
```gherkin
Given a page whose frontmatter uses OKF block sequences and a quoted title
When the explorer extracts its metadata
Then the tags are the block sequence entries, unquoted
  And the source is the first sources[].resource, unquoted
  And the page type is the frontmatter "type", not a filename guess
```
Implemented by `TestExtractPageMetaReadsOKFFrontmatter`,
`TestExtractPageMetaUnquotesFrontmatterScalars`,
`TestExtractPageMetaReservedFilenamesWinOverFrontmatter`,
`TestExtractPageMetaIgnoresTypeInTheBody`.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/wiki/okf.go` | Created | OKF version, actor, `generated`/`sources` writers, YAML scalar quoting |
| `internal/wiki/okf_log.go` | Created | §9 date-grouped log writer, shared by both modules |
| `internal/wiki/lint.go` | Modified | `type` is the required field; staleness reads `generated`/`stale_after`; dotted keys are visible to the field scanner |
| `internal/wiki/crossref.go` | Modified | `isBundlePageLink` — a repo path is not a cross-reference |
| `internal/wiki/toon.go` | Modified | Title-first result rows; `FormatBM25ResultsTOON`; bounded opt-in preview |
| `internal/wiki/bm25.go`, `internal/wiki/search.go` | Modified | `DocType` carried into `BM25Result` so a title-first row can show it |
| `internal/wiki/autolink.go` | Modified | Doc comment corrected — it emits markdown links, not wikilinks |
| `internal/knowledge/wiki.go` | Modified | `generated`/`sources`/`stale_after`; index without frontmatter; §9 log; quoted scalars |
| `internal/memory/wiki.go` | Modified | Same, plus the index page and log that the first OKF pass never converted |
| `internal/uiserver/wiki_handler.go` | Modified | Reads OKF frontmatter; page type comes from `type` |
| `internal/ui/src/components/wiki/WikiMarkdown.tsx` | Modified | `wikiPageTarget` — only a bundle page becomes a navigation button |
| `internal/ui/src/components/wiki/WikiExplorerPage.tsx` | Modified | Cross-reference preamble stripper; concept count no longer assumes `entity` |
| `internal/mcpstdio/tools_knowledge.go`, `tools_memory.go`, `tools_wiki.go` | Modified | Title-first output, `preview` opt-in, tool descriptions say so |
| `internal/mcpstdio/server.go` | Modified | `wantPreview` |
| `internal/knowledge/rule.go`, `internal/memory/rule.go` | Modified | Skill and mandate state that search answers with titles |
| `internal/knowledge/okf_conformance_test.go` | Created | §11 conformance checked by parsing generated output |
| `internal/memory/okf_conformance_test.go` | Created | Same for the memory bundle, including index and log |
| `internal/wiki/okf_test.go` | Created | Link classification, frontmatter writers, log grouping, title-first formatting |
| `internal/uiserver/wiki_okf_frontmatter_test.go` | Created | Explorer reads the OKF shape |

## Technical Debt

- [ ] The Hub has no `knowledge` artifact for OKF. Every future session will re-fetch the spec
  from GitHub or, worse, answer from model memory. Submitting one is the fix.
- [ ] The AI-answer citation protocol in `internal/wiki/search.go` and `multi_search.go` still
  uses illustrative page-name and source-scoped page placeholders. Deliberately left alone: it is a PROMPT
  protocol between the searcher and the model, never written to a bundle, and the
  module-scoped source/page form has no OKF equivalent. Changing it means changing
  the answer parser and `preprocessWikiLinks` in the UI at the same time.
- [ ] `docs/tasks/2026-08-20-okf-wiki-compliance.md` claims 100% compliance and a full green
  test run. Both were true of the tests and false of the claim: the tests were updated to the
  new output, which is not the same as checking the output against the spec. That task log
  needs a correction entry pointing here.
