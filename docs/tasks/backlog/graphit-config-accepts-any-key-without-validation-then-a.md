# `graphit config` accepts any key without validation, so a typo writes garbage and the symptom shows up far from the cause

## Symptom

`graphit config <chave> <valor>` writes any key, known or not, and prints success. Two distinct failures fall out of that:

1. **Nonexistent key from a typo.** `graphit config ast.grammars_blacklst yaml` (missing the `i`) writes `{"config":{"ast":{"grammars_blacklst":"yaml"}}}` into `graphit.lock.json`, reports success, and turns nothing off. The user concludes the feature is broken. The same holds for `ast.index_doc`, `knowledge.doc_dir`, `dream.idle_timout`, etc.
2. **Nonexistent subcommand read as a key.** Already documented in the memory `A CLI de config é graphit config <chave> <valor>`: `graphit config set X Y` writes the key `set` with value `X`. The proof is in the repository itself — this project's `graphit.lock.json` carries `"config": {"get": "knowledge.docs_dir"}`, inert residue of a badly invoked `graphit config get` in an old session.

This has always existed. What changed on 2026-08-24 is the **blast radius**: `ast.grammars_blacklist` and `ast.grammars_whitelist` landed (docs/tasks/disable-grammars-by-config.md), whose intended effect is the ABSENCE of nodes in the graph. A typo in those produces no error at all — it produces "the language I turned off is still being indexed", which is indistinguishable from a stale index, from a stopped daemon, and from a bug in the filter. The other keys at least fail on the visible side (a wrong docs directory gives an empty wiki).

## Where it is

- CLI handler: `cmd/graphit/commands/` — look for the `config` command (`newConfigCmd`, or equivalent; `graphit config --help` shows the real signature `graphit config [--global] [--get|--unset|--list|--secret] [chave] [valor]`).
- Read/write: `internal/config/config.go` — `SetConfigValue`, `GetConfigValue`, `SetGlobalConfigValue`, `LoadProjectConfig`.
- The resolvers are the source of truth for the keys that DO exist: the `Resolve*` functions in `internal/config/config.go` (`ResolveDocsDir`, `ResolveASTQueriesDir`, `ResolveASTGrammarsBlacklist`, `ResolveASTGrammarsWhitelist`, `ResolveAstIndexDocs`, `ResolveBacklogDir`, `ResolveDreamReportsDir`, `ResolveKnowledgeExtensions`, `ResolveSearchRerank`, `IsModuleDisabled`, …) plus the dynamic `modules.<name>` pattern. Today that list exists nowhere as data — each resolver carries its own string literal.

## What to do

1. Create a **registry of known keys** in `internal/config` — a table of `chave → {descrição, default, dinâmica?}`, with `modules.*` marked as a dynamic prefix. Make each `Resolve*` read its name from the registry instead of repeating the literal, so the table cannot diverge from the resolvers (a test comparing the two sets is even better).
2. In the CLI, when **writing** an unknown key: fail with a nonzero exit code and suggest the closest one by edit distance (`did you mean ast.grammars_blacklist?`). Writing is the operation where validation is worth it, because it is the only one in which the user asserts that the key exists.
3. When **reading** (`--get`) an unknown key: warn on stderr without failing — reading a nonexistent key is a legitimate question.
4. Consider an escape hatch flag (`--force`) to write a key outside the registry, so as not to block anyone preparing config for a future version. If it exists, it must say in the output that the key is not recognized by this binary.
5. Make `graphit config --list` flag the keys present in the lockfile that the registry does not know — that is how residue already written (like this repository's `"get"`) becomes visible instead of invisible forever.
6. Update `docs/guides/cli_reference.md` and `docs/specs/config_module.md`, and remove from the troubleshooting entry *A whole language is missing from the AST code graph* (docs/guides/troubleshooting.md) the step that today says "check the spelling letter by letter" — it only exists because this validation does not.

## How to know it worked

- `graphit config ast.grammars_blacklst yaml` exits with an error, writes nothing to the lockfile, and suggests `ast.grammars_blacklist`.
- `graphit config set ast.index_docs true` exits with an error instead of writing the key `set`.
- `graphit config ast.grammars_blacklist yaml` keeps working the same.
- `graphit config modules.dream false` keeps working (dynamic prefix).
- A test that walks the registry and asserts that every key has a matching `Resolve*`, and vice versa — so that the next new key cannot forget the registry.
- `graphit config --list` in a project whose lockfile has `"config":{"get":"knowledge.docs_dir"}` flags that entry as unknown.

## What has already been ruled out

- **Do not** validate only on read: the damage is on write, and a key already written wrongly never gets asked about again.
- **Do not** derive the keys by reflecting over the lockfile JSON: the lockfile is the place where the garbage already is — it does not know what is legitimate.
- **Do not** make the grammar lists strict (rejecting an unknown grammar name) to compensate: it was deliberately decided that an unmatched name is inert, because the lists are read in processes that may not have the grammar pack installed, and rejecting would turn "I haven't installed that grammar yet" into a hard failure of the whole index. The problem is the KEY name, not the value.
