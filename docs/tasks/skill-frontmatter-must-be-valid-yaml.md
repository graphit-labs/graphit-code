# Skill frontmatter has to be valid YAML: an unquoted `": "` made every skill invisible to Kiro

## Date
2026-08-12

## Problem
In Kiro, the agent had **no skills available at all**. Not a stale one, not a
partially loaded one — the IDE's skill list was empty while
`.kiro/skills/graphit-{ast,hub,improvements,knowledge,memory}/SKILL.md` all existed
on disk, in the documented location, with the documented `name` and `description`
fields. The mandate in `AGENTS.md` still arrived (rules are a separate path), so the
agent was being told to read five skills the IDE never offered it.

Nothing reported the failure. No entry in `~/.kiro/logs/*/kiro.log`, no diagnostic in
the IDE, and the same files loaded fine in Claude Code — which is what made this look
like a Kiro configuration problem rather than a defect in the files we generate.

## Root Cause
The frontmatter was **invalid YAML**, and had been since these descriptions were
written.

Every module builds its own frontmatter by string concatenation and emits the
description as a *plain* (unquoted) scalar:

```go
frontmatter := "---\nname: " + astSkillName + "\ndescription: AST Code Exploration … Use for: finding functions, …\n---\n\n"
```

A plain YAML scalar may not contain `": "` — the parser reads the colon as the start
of a nested mapping. Every one of the five descriptions contains at least one, because
they are written as prose for a model: `Use for: …`, `Use when: …`,
`MANDATORY: After ANY code change…`, `MANDATORY at conversation start: …`.

Parsed with the exact library Kiro ships
(`/usr/share/kiro/resources/app/node_modules/yaml`):

```
YAMLParseError: Nested mappings are not allowed in compact mappings at line 2, column 14
```

PyYAML rejects it the same way (`ScannerError: mapping values are not allowed here`).
So the failure mode is not "the skill loads with a truncated description" — the
document does not parse, the skill has no metadata, and a skill with no metadata is
not discovered. **Kiro's loader is strict; Claude Code's is lenient.** The same bytes,
two outcomes, which is why this survived so long: the IDE most used here hid it.

Location and naming were never the problem. The Kiro adapter
(`NewKiroAdapter`, `internal/hub/adapters/ide/adapters.go`) already writes
`.kiro/skills/<name>/SKILL.md` in folder mode, and each folder name already matched
its `name` field.

## Changes

### `internal/hub/adapters/ide/adapters.go`
- `SkillFrontmatter(name, description string) (string, error)` — the single place that
  builds the frontmatter block for a managed `SKILL.md`. It marshals a
  `skillFrontmatter` struct with `gopkg.in/yaml.v3` and wraps the result in `---`
  delimiters. **No escaping is done by this repository.**

The first version of this fix hand-rolled a `yamlDoubleQuoted` helper — quote, escape
`\`, `"`, `\n`, `\r`, `\t`, close quote. It produced correct output for today's five
descriptions and was the wrong design anyway: deciding which values need quoting, and
in which style, is a YAML problem with a long tail (leading `%`, a value that looks
like `yes`, `no`, `null` or a number, a trailing space, a `#`), and every case not
anticipated fails the same silent way this bug did. Replaced on review with the
marshaller, which owns that decision.

Measured before committing to it, because the reason to distrust `yaml.Marshal` here
was real — the emitter folds long lines in some configurations, and a folded
description would be valid YAML that a line-oriented reader truncates:

- A 473-character description comes out on **one line**. `yaml.Marshal`,
  `NewEncoder`+`SetIndent(2)`, and a `yaml.Node` with `DoubleQuotedStyle` all emit two
  lines total (`name`, `description`). No folding at these lengths.
- The style is chosen per value: `'single-quoted'` for prose containing `": "`, and
  `"double-quoted"` with `\"`, `\\`, `\n`, `\t` escapes when the value actually needs
  them. Round-trips byte-identical, including a trailing space.

So the output changed style — `"…"` in the hand-rolled version, `'…'` from the
marshaller — and that is not a regression: the only hand-written frontmatter reader in
this repo, `wiki.ExtractSummary` (`internal/wiki/docutil.go:43`), does
`strings.Trim(desc, "\"'")` and accepts both. It never sees these files anyway; it
reads the docs tree and the memory store, and `SKILL.md` is in neither.

### `internal/{ast,memory,knowledge,hub,improvements}/rule.go`
Each `InstallSkill` now calls `ide.SkillFrontmatter(<skillName>, "<description>")` and
propagates its error instead of assembling the block inline. The descriptions
themselves are unchanged — this is about how they are serialized, not what they say.
`internal/knowledge` keeps interpolating the resolved `docs_dir` into its description;
the marshaller now covers whatever that value turns out to be.

The error return is plumbing for a case that cannot occur today — marshalling a struct
of two strings does not fail — and it is there because the alternative is discarding
it. A future `SkillFrontmatter` that cannot represent its input should make
`graphit sync` fail loudly, not write a file the IDE will silently ignore.

### Regenerated skill files
`.agents/`, `.kiro/` and `.claude/` — the three IDEs in `graphit.lock.json` — were
regenerated through the real generators. Two kinds of diff came out:

- **All five skills**: the `description:` line, now quoted.
- **`graphit-ast` only**: ~80 lines of body, because the checked-in copies were stale
  relative to `ASTRuleContent()`. They predate the recent `CALLS`/stub-semantics
  rewrite; regenerating brought them in sync. Nothing about that is caused by this
  change — it is the drift that exists whenever the rule text moves ahead of the last
  sync.

## Verification
- `go build -tags fts5 ./...` clean. `go vet` clean on the changed packages (the
  pre-existing `gofmt` complaint in `internal/hub/rule.go:468` is untouched and
  unrelated).
- `go test -tags fts5 -count=1 ./internal/hub/adapters/ide/ ./internal/hub/
  ./internal/improvements/ ./internal/memory/` — green;
  `-run 'Rule|Skill|Frontmatter' ./internal/ast/ ./internal/knowledge/` — green. No
  existing test asserted the old frontmatter text, so nothing needed updating.
- **The check that matters**: all 15 regenerated files parsed with Kiro's own bundled
  `yaml` module. Every one yields exactly two keys (`name`, `description`), `name` equal
  to its folder, and a description between 1 and 1024 characters (the documented cap;
  the longest is 571). Before the change, all 15 threw.
- In-session proof: `disclose_context("graphit-memory")` — the Kiro tool that activates
  a skill by name — returned the full skill body immediately after regeneration, having
  found nothing before it. The *list* of available skills is rendered at session start
  and stays empty until the next session; resolution by name was already fixed.

## Notes
- **Kiro discovers skills when a chat session starts.** Regenerating the files does
  not retro-fit the running session; the skills appear in the next one.
- The 1024-character cap on `description` is real and these are close enough to it
  (571 max) that a future edit could cross it. Nothing enforces it in the generator.
## Follow-up — the four deferred items, closed the same day

The first pass left four items on a dream subject. They were closed on request
immediately afterwards, in the same session.

### A. A guard that actually bites

`internal/hub/adapters/ide/frontmatter_test.go` (new) exercises `SkillFrontmatter` with 21
values chosen because a hand-written quoter gets them wrong: prose with `": "`, a `"`, a `\`,
leading `%`, `#`, `-`, `?`, `&`, `*`, plain scalars that resolve to bool/null/number/date,
leading and trailing whitespace, a tab, a newline, non-ASCII. Each asserts the description
round-trips **byte-identical** through YAML. Two more tests cover the block's shape and the
rejections from item B.

`cmd/graphit/commands/managed_skills_frontmatter_test.go` (new) is the integration half: for
**every supported IDE × all five modules** — 35 subtests — it installs through the real
generator, reads the file back the way an IDE would, and asserts the frontmatter parses with
exactly two fields, `name` equal to the installed directory, and the limits from item B. It
lives in `cmd/graphit/commands` because that package already imports all five modules; the
`ide` package cannot, since the modules import *it*.

A second test there asserts the descriptions **still contain `": "`**. Without it, the
integration test degrades quietly into proving the content became bland rather than that the
quoting works.

**Both were seen failing.** Patching `SkillFrontmatter` back to string concatenation turns 16
subtests red in the unit file and makes the integration test report *"frontmatter is not valid
YAML — the skill would be invisible to a strict IDE: yaml: line 2: mapping values are not
allowed in this context"*. Worth recording which hostile values do **not** fail under
concatenation: `"`, `\`, `yes`, `1024` and `2026-08-12` all survive unquoted, the last three
because the target field is typed `string`. A test built only from those would have been
green against the bug.

### B. The limits are enforced, not documented and hoped for

`SkillFrontmatter` now validates before marshalling and returns an error otherwise:

- `name` — non-empty, `^[a-z0-9]+(-[a-z0-9]+)*$`, at most `MaxSkillNameLength` (64).
- `description` — not blank, at most `MaxSkillDescriptionLength` (1024).

Counted in **runes**, via `utf8.RuneCountInString`: these descriptions carry em dashes, so a
byte count would report ~1.1× the real length and reject a description that is inside the
limit.

The error path was chosen over silent truncation for the same reason the whole bug existed: a
value outside these limits is dropped by the IDE without a word, and an error at generation is
the only place it can still be seen. The five callers already propagate it, so it surfaces as a
failing sync.

One consequence is a real behavior change: `brand.SkillDirName` is `Brand + "-" + module`, so a
white-label build with `Brand = "MyCorp"` produces `MyCorp-ast`, which is **not** a valid skill
name. That build now fails loudly at skill installation. It was already broken — strict IDEs
would refuse those skills — but it failed silently before.

### C. Where an agent writes the frontmatter itself

Two places instruct an agent to author a `SKILL.md`, and neither goes through
`SkillFrontmatter`:

- `internal/dream/prompt.go` — the skill crystallization protocol said only
  "**Name & Description** (frontmatter)". It now names the `name` and `description` contracts,
  says to quote the description and why, and shows a valid block.
- `internal/improvements/rules.go` — Step 3b's folder-based artifact example had no
  frontmatter at all. It now opens with one, quoted, and carries the same warning.

This changes the improvements skill's body, which is why `.agents/`, `.kiro/` and `.claude/`
carry an 11-line diff for `graphit-improvements` beyond the frontmatter line.

### D. Reading frontmatter with a parser instead of a guess

`internal/wiki` had three hand-written readers. `ExtractTitle` and `ExtractSummary` did
`strings.Trim(value, "\"'")` — which tolerates quotes but never unescapes, so a single-quoted
`the project''s docs` reached the summary with the doubled apostrophe intact — and
`ReadFrontmatterField` did not strip quotes at all.

Added to `internal/wiki/helpers.go`:

- `FrontmatterBlock(content)` — the leading block's text, or `ok=false`.
- `FrontmatterField(content, field)` — one scalar, read from a `yaml.Node` so the value is the
  scalar's **literal text**: quoting and escaping resolved by the parser, and no YAML type
  resolution, so a content hash that happens to be all digits stays a string. `ok=false` for
  absent, null and non-scalar, which keeps "field missing" distinct from "field empty".

All three readers now try YAML first and **fall back to the old line scan** on `ok=false`. The
fallback is load-bearing rather than defensive: wiki pages are themselves written with
hand-assembled frontmatter (`memory/wiki.go`, `knowledge/wiki.go`), so a page whose title
contains `": "` does not parse — see the note below — and the scan is what keeps it readable.

While centralizing the truncation, `truncateSummary` now counts runes. The previous
`desc[:200]` could cut a multi-byte character in half in a generated page.

### What this follow-up did NOT fix

**Wiki and memory pages still assemble their own frontmatter**, with `fmt.Fprintf(&b, "title:
%s\n", title)` and friends — `internal/memory/wiki.go`, `internal/memory/memory.go`,
`internal/knowledge/wiki.go`. The proof is delicious: the memory page written for *this very
bug* has invalid YAML frontmatter, because its title contains `": "`.

It was left out deliberately. Unlike the skill block, those blocks are not two string fields:
`confidence: %.2f` must not become `0.9`, `tags: [a, b]` is a flow sequence assembled as text,
and several tests fix the current output byte for byte. It also needs a migration story, since
`content_hash` is derived from the *source* document — so fixing the writer does not rewrite
pages whose sources have not changed; only a reset reindex heals them. That is its own change,
and its own dream subject.

## Verification (follow-up)

- `go build -tags fts5 ./...`, `go vet` and `gofmt` clean on every changed file. The one
  remaining `gofmt` complaint, a double blank line in `internal/dream/prompt.go`, predates this
  work and was left alone.
- Green: `internal/{wiki,hub,hub/adapters/ide,dream,improvements,knowledge,memory,wikisvc,chat,daemon,mcpstdio,uiserver}`
  and `cmd/graphit/commands`. That set covers all 24 files that import `internal/wiki`,
  resolved from the AST graph rather than guessed.
- The 15 skill files regenerated through the real generators and re-validated with Kiro's own
  bundled `yaml`: two keys, `name` equal to the folder, description within 1..1024, and still
  containing `": "`.
