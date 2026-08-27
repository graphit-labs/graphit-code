# _INLINE_0_ accepts any key without validation, so a typo overwrites data and the symptom appears far from the cause.

## Sintoma

The _INLINE_1_ writes any key, known or not, and prints success. Two distinct failures fall from there:

1. **The nonexistent key due to typo.** `graphit config ast.grammars_blacklst yaml` (the missing `i`) writes `{"config":{"ast":{"grammars_blacklst":"yaml"}}}` into `graphit.lock.json`, reports success, and does not disable anything. The user concludes that the feature is broken. This applies equally to `ast.index_doc`, `knowledge.doc_dir`, `dream.idle_timout`, etc.
2. **Subcommand nonexistent read as key.** Already documented in memory `The config CLI is graphit config <key> <value>`: `graphit config set X Y` writes the key `set` with value `X`. The proof is in the same repository — the `graphit.lock.json` of this project loads `"config": {"get": "knowledge.docs_dir"}`, inert residue from an `graphit config get` that was never invoked in a previous session.

This has always existed. What changed on August 24, 2026 was the **explosion radius**: they entered `ast.grammars_blacklist` and `ast.grammars_whitelist` (docs/tasks/disable-grammars-by-config.md), whose intended effect is our absence from the graph. A typo in them does not produce any error — it produces "the language I turned off continues indexed", which is indistinguishable from an old index, a daemon that's stopped, and a bug with the filter key. The other keys at least fail for the visible side (a wrong directory of docs gives a wiki with nothing).

Where is it?

- Command Handler for CLI: `cmd/graphit/commands/` — search for the command `config` (`newConfigCmd`, or equivalent; `graphit config --help` shows the real signature `graphit config [--global] [--get|--unset|--list|--secret] [chave] [valor]`).
- Read/Write: `internal/config/config.go` — `SetConfigValue`, `GetConfigValue`, `SetGlobalConfigValue`, `LoadProjectConfig`.
- Resolvers are the list of true keys that EXIST: functions `Resolve*` of `internal/config/config.go` (`ResolveDocsDir`, `ResolveASTQueriesDir`, `ResolveASTGrammarsBlacklist`, `ResolveASTGrammarsWhitelist`, `ResolveAstIndexDocs`, `ResolveBacklogDir`, `ResolveDreamReportsDir`, `ResolveKnowledgeExtensions`, `ResolveSearchRerank`, `IsModuleDisabled`, …) plus the dynamic pattern `modules.<name>`. Today, this list does not exist anywhere as data — each resolver loads its string literal.

What to do

1. Create an **in-memory key registry** in `internal/config` — a table of `key → {description, default, dynamic?}`, with `modules.*` marked as dynamic prefix. Have each `Resolve*` read the name of the registry instead of repeating the literal to prevent divergence from resolvers (a test that compares both sets is even better).
2. In CLI, when **writing** an unknown key: fail with a non-zero exit code and suggest the closest by edit distance (__INLINE_45__). Writing is the operation where validation holds true because it's the only one where the user asserts that the key exists.
3. When reading (`--get`) an unknown key: warn stderr without failing — reading of nonexistent keys is a legitimate question.
4. Consider a flag for escape (__INLINE_47__) to write a key outside the registry, not to block those preparing configuration for a future version. If it exists, it should be output in the log that the key is not recognized by this binary.
5. Mark `graphit config --list` as present keys in the lockfile that the registry does not know — this way, already recorded (as the `"get"` of this repository) residues are visible instead of invisible forever.
6. Update `docs/guides/cli_reference.md` and `docs/specs/config_module.md`, and remove from troubleshooting entry *A whole language is missing from the AST code graph* (docs/guides/troubleshooting.md), the step that says "check spelling letter by letter" — it exists only because this validation does not exist.

How do you know it worked?

- **INLINE 52** exits with an error, does not write anything to the lockfile, and suggests **INLINE 53**.
- **INLINE 54** exits with an error instead of writing the key **INLINE 55**.
- **INLINE 56** continues functioning as usual.
- **INLINE 57** continues functioning (prefix dynamic).
- A test that traverses the log and asserts that each key has a corresponding `Resolve*`, and vice versa — to ensure the next new key does not forget about the registration.
- **INLINE 59** in a project where the lockfile has `"config":{"get":"knowledge.docs_dir"}` mark this entry as unknown.

What has already been discarded

- **Do not** validate solely on reading: the damage is in writing, and a key already written incorrectly cannot be reasked.
- **Do not** deduce keys by reflection on the JSON of the lockfile: the lockfile is where the garbage has already been thrown away; it does not know what is legitimate.
- **Do not** make strict grammatical lists (rejecting unknown grammar names) to compensate: this was deliberately decided that a name without a corresponding entry is inert, because the list is read in processes that may not have the grammar pack installed, and rejecting would turn "I haven't installed this grammar" into an irreparable failure of the entire index. The problem is the key's name, not its value.
