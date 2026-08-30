# Config in `.js`/`.ts` stops being invisible — and the cost was measured, not estimated

## The finding

`graphit ast index` reported two files under "Empty (0 entities)":
`internal/ui/postcss.config.js` and `internal/ui/tailwind.config.js`. Both are just
`export default { … }`.

Every pattern in `javascript.yaml` requires a NAME — a function, class, or variable
declaration, or an import/export with a source string. An anonymous default export of
an object literal declares no name at all, so nothing matched. The proof is right next
door: `eslint.config.js`, same folder, **did not** show up on the list, because it uses
`import … from '…'`.

The comparison that turns this into a real defect rather than a minor detail:
`tsconfig.json`, in the same directory, yields 20 `Pair` + 20 `Value`. The same kind of
configuration was reachable or not **based solely on the file extension**.

## Three iterations, and what each one taught

**1st — capture every pair with a literal value.** This solved it, and cost **+37%
nodes in `.tsx`**. The sample revealed the problem: half was real content (the
`pkb → sql`, `plsql → sql` table from CodePanel), half was a call option repeated per
call site — `behavior: 'smooth'`, `block: 'center'`, `overflowX: 'auto'`.

**2nd — anchor on the exported object or a const.** Zero noise, cost **+7.8%**… and
**did not fix the reported problem**. In a real config, the literals sit three or four
levels down (`theme.extend.colors.border`), and the anchor only catches direct
children. `tailwind.config.js` stayed empty.

The test I had written passed because the fixture was flat — the shape I imagined, not
the one that actually exists. **Testing against the real file is what caught it.** The
`TestTheConfigFilesThatWereEmptyAreNotEmptyAnyMore` test exists so that this doesn't
happen again: it reads `internal/ui/*.config.js` from disk.

Tree-sitter has no descendant wildcard and no structural negation, so "any literal
pair, except the ones inside a call" isn't expressible. The choice between the two
options was the Engineer's, with both measurements on the table: **no anchor**.

**3rd — the key of a structural value is also content.** `postcss.config.js` was still
at zero, and the Engineer pointed out: even with only empty objects, the graph should
still know that `autoprefixer` exists. That's correct — `plugins: { autoprefixer: {} }`
names a plugin, and `{}` means "no options," not "nothing here." Capturing the key
(without a value, since there's no literal) cost **+2.3 points** (37.0% → 39.3%). Array
items — the `content: [...]` globs — came **for free**, just like `json.yaml` already
did.

## Result

| file | before | after |
|---|---|---|
| `postcss.config.js` | 0 entities | 3 (`plugins`, `tailwindcss`, `autoprefixer`) |
| `tailwind.config.js` | 0 entities | 123 |

Cost: **+39.3% nodes in `.tsx`**, +35.7% in `.ts`. Only in these languages — here that's
52 of 703 files, so the global effect is small. In a purely front-end project it would
be material, and it's worth re-measuring there.

## `require()` became a dependency

Found along the same path: `require('tailwindcss-animate')` arrived as a call to a stub
named `require`, with the module name unreachable — so "what this project depends on"
was missing all of CommonJS. It's now an `Import`, via the `#eq?` predicate pinning the
callee, with an auxiliary `@_fn` capture that doesn't become an entity.

## The containment guard flagged this twice, and was right both times

Declaring `context_types` in `javascript.yaml` and `tsx.yaml` — which had none — made
the test start checking ALL of their containers, and revealed that
`function_declaration`, `method_definition`, and similar nodes had never been contexts
there: parameters were being attributed to whatever surrounded them, or discarded.
Fixed at the same time.

`pair` stayed exempt FOR A REASON: unlike `json.yaml`, which captures every pair and
therefore declares `pair` as a context, here only the literal-value pair gets captured,
and a captured pair never contains another. The nearest named declaration is what owns
them — that's why `lexical_declaration` IS a context: `const X = { … }` owns its pairs.

## A trap I repeated

Inserting `context_name_paths:` right after the first line of `context_types:`
swallows the rest of the entries into it — the YAML stays valid, and the entries turn
into key→label inside a map that expects key→path. I had already recorded this in
memory while working on postgresql/tsql, and fell into it again, this time in
typescript. When editing these blocks via script, rewrite the whole block.

## Progress Log

- 2026-08-16 — Pair/Value for configuration objects and lookup tables,
  structural-value key, array item, and `require()` as Import, in `javascript`,
  `typescript`, and `tsx`. `shardCacheVersion` 6 → 7. Six tests, one of them against
  the real files. Full suite green, `make lint` 0 issues.
