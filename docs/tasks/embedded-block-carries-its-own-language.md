Embedded block loads its own language; three repetitions in what the agent reads

## Contexto

Auditoria de uso real do framework num corpus grande de banco de dados (PL/SQL Oracle +
XML configuration flow). The agent that performed the analysis reported where the tools were located.
ajudaram e onde falharam. Este log cobre o que foi verificado, corrigido e adiado.

The first find in the report was the most serious, and his diagnosis was wrong — as stated.
The verification changed the root cause. The others were measured one by one before turning into a change.

The primary flaw: resolved by using the wrong reference block.

### Sintoma

In an embedded SQL configuration with XML, `MATCH (a)-[r]->(t:Table)` filtering by
The ``r.source_file`` of XML returned **no lines**. The natural reading would be, "this flow doesn't"
"It's not even in the same league as that." - That is the opposite of the truth.

The symptom was not missing junctions. The junctions existed, as self-loops `File → File`:
SELECTS 2617, UPDATES 545, INSERTS 181, DELETES 25. Aresta presente e **invertida**, que
It's worse than absent: nothing announces this as a gap.

### Causa raiz

The measurement that isolates the problem:

```cypher
MATCH (t:Table) RETURN t.lang, count(*)     -- plsql, 10674
MATCH (n) WHERE n.path STARTS WITH '<dir do xml>/' RETURN n.lang, label(n), count(*)
Everything XML - including Cursor, Procedure, and Variable, which only the PL/SQL parser produces
```

Tudo o que o parser embutido produzia ficava carimbado com a linguagem do arquivo
HOSPEDEIRO. `mergeParsedInto` dobrava o parse interno no externo concatenando listas,
Without any trace of where they came from. Two guards at the wheel then failed together:

1. `resolveNamed` exige `d.lang == lang`. É proposital e correto — o `fill()` de um
``.tsx`` cannot call the `Go` function itself, but the reference says ``xml``, and...
The declarations are INLINE_0, so she never married.
2. `refRule` escolhe a `TargetRule` POR LINGUAGEM, e as regras de DML com
They live in `plsql.yaml` / `sql.yaml`. `xml.yaml` does not declare anything.
Then the fallback became `TargetFallbackStub`, which returns `(ref.Path, LabelFile)`.

Neither of them is isolated bugs. The bug is the wrong language arriving in them.

Correction

- `internal/ast/parser.go`: `Entity`, `CallInfo` e `ReferenceInfo` ganharam `Lang` — a
Language that GENERATED the item is empty when it's from the file.
- `internal/ast/treesitter_embedded.go`: `mergeParsedInto` carimba `inner.Language` nas
Three lists, filling in only what is empty for the innermost block to win in.
  aninhamento. Helper `langOr`.
- `internal/ast/parse_cache.go`: `Lang` em `cachedCall` e `cachedReference`.
- `internal/ast/cache_convert.go`: propaga com `langOr(item.Lang, pf.Language)`.
- `internal/ast/rebuild_index.go`: `resolveRefTarget` faz `lang = langOr(ref.Lang, lang)`;
The three callers of `resolveCallee` pass `langOr(call.Lang, fe.entry.Language)`.
- `internal/ast/shard_cache.go`: `shardCacheVersion` 4 → 5.

It applies to any embedded language, not just SQL in XML.

Tests - it's the point

`internal/ast/embedded_lang_resolution_test.go`, quatro casos. Verificado que o teste
primary **failure without correction**, exactly as the production symptom (0 lines).

Um teste sobre `pf.References` passa com o defeito inteiro no lugar, e foi assim que
isto sobreviveu a duas rodadas: `TestEmbeddedANTLRBlockProducesDMLEdges`, apesar do
Name never looked at the graph. Includes **negative control** — a reference without INLINE_0
She cannot cross the language barrier without it - otherwise, even if she fails the test.
resolution would ignore language entirely.

Three cuts throughout the entire session

### `ast_schema` agrupa labels que compartilham a lista de propriedades

Almost all labels are entities and carry the same 16 properties; only `File`.
**INLINE_0** and **INLINE_1** differ. They were approximately 25 repetitions of the same list. Now, the forms are different.
Unique ones come out one per line (the difference between INLINE_0 and INLINE_1 of an entity is
justamente o que faz query estourar) e as compartilhadas saem uma vez, nomeando os
Labels. No information gets lost. `internal/ast/schema.go`, test in
`schema_shared_shape_test.go`.

Agents.md: A policy is invariant once stated.

Five modules repeated the same six phrases (precedence, CLI prohibition, list of...).
Natives' Tools: "If you're unsure, apply it," "Reapply with each request."
Clause of Integrity. **Measured: ~3.228 bytes, 18.6% of the file, were copies beyond the original.
Here's the translation:

First, Isolated should be inline (INLINE_0), preceding all blocks.
He directs what varies: domain, skill, triggers, tools.
`internal/hub/adapters/ide/mandate.go`. O teste antigo afirmava o desenho antigo e foi
Substituted by two: The block carries what varies and **not** politics; and the preamble to.
It affirms exactly once—without this second half, canceling out politics would be impossible.

Skill of AST: When hybrid is sought, it's noise.

In a corpus without prose and with rigid convention of name (`PRC_`, `PCK_`, `IX_`), the two
The sides of the hybrid rank over text and don't have what separates them: the observed result.
foram quinze scores achatados em 0,03–0,05. A skill agora nomeia o caso e o sinal para
Recognize him (scores plans, top-1 not better than top-10), and sends him straight to
Cypher com `STARTS WITH` no prefixo. `internal/ast/rule.go`.

The "leaking label" was not an indexing defect; it was a missing instruction.

Relatado como `PRC_X` voltando com label `Value` em vez de
They are two legitimate entries in the same file: INLINE 1 on line 2 and one.
On line 9, which is the literal of string `:= 'PRC_X'`
Initializing `v_nome_progr`_. Occurs with 338 procedures of the corpus and does not break.
resolution — the `target_rules` of `plsql.yaml` restricts `CALLS` to
Here is the translation:

"**INLINE_0**, and edges **INLINE_1** reach procedures."
"sombreadas" normalmente.

The first version of this log classified it as an agent query trap.
Errado.** O agente tinha lido a skill — e a skill MANDA fazer exatamente a query que
produz o resultado confuso: `Phase 2: Pre-search (Grounding)` instrui
`MATCH (n) WHERE toLower(n.name) CONTAINS ...` sem label, e a tabela multi-label traz a
linha "Anything — full discovery". Em lugar nenhum ela dizia que existem labels
appointed by **content**. Who followed the instruction received a result that was

---

This is already English, so no changes were made.
The instruction was not prepared: this is a defect of skill.

The entire class is raised instead of invented: INLINE_0, INLINE_1, and INLINE_2 come
From 37 grammars sent in, and four were already documented.
They have it equal to the actual content.

Closed instruction holes in the same round

Applying the same criterion— if the agent reads and makes a mistake, it's missing instruction—

This is already in English. No translation needed.

The query returns an empty result or only readers. "Nobody writes in this table."
A conclusion with consequences, and the mode of failure is not missing link: it's the missing link.
He resolved for another node. The skill now inspects `label(a)/label(b)`.
Before concluding absence, and explains that `File → File` means target.
Unresolved
Without columns or uniqueness constraints. Forbidden from inferring "the database does not guarantee this."
A query that did not find an unique index - the graph does not index it, so the empty set.
The proof is in the pudding. Before making the assertion, read the DDL with the tool of source.
Cold Start with an Empty Memory and a Query That Doesn't Exist
They were indistinguishable in their pursuit. `graphit_memory_list` reads the store directly.
Resolve in a call — instruction to use in the first empty search, not on
  terceira.

## Segunda rodada: os itens adiados, feitos

Assignment to Host Entity

The statement of an embedded block has its origin in the entity that it HOSTS, not
o arquivo. `attributeToHostEntity` + `hostEntityAt` em `treesitter_embedded.go`, rodando
Before the merge, while the block position is still in hand—this is the embedded parser's role.
Last step of the file, then the host entities already exist with absolute lines.

Innermost por span, porque documento aninha; content-named labels (`Value`,
Excluded from `AttributeValue`, `Text`, and `Comment`, otherwise the text loading node.
The statement would always be the most internal and the origin would be the very text of the statement itself.
Statement. By line and name, because INLINE_0 is a map and maps do not have order.

It's half that dictates the grammar of the project: the engine doesn't know what the host is
Models - stage, job, handler are all just declared entities by some grammar.
Because of not knowing, he attributes any one.

Corrected on August 19, 2026, and what is written above describes the defective version.
> "Innermost por span" era pouco: o chamador passava o DESLOCAMENTO como linha do bloco, uma
Above it, then the origin turned into the brother above— in an indented XML, the `<key>` that
> antecede o `<value>`. E "innermost que cruza a linha" foi trocado por "innermost que CONTÉM
The block, strictly speaking, because in a data grammar, the span of an entity ends at the start.
> tag. Ver `docs/tasks/embedded-block-host-must-contain-the-block.md`.

### Índice com tabela, colunas e unicidade

`Index` era um nome e nada mais. Agora carrega:

Table, as `REFERENCES` exits from index (`create_index`) entered
  `context_types`);
Columns covered, in order, which is semantic in an indexed compound;
- a **unicidade**, no `value` — justamente a propriedade que a auditoria encontrou vazia.

`INLINE_0` is a keyword, not a rule, and therefore required a new capability in ```
Motor: It falls into the TOKEN when no rule fits. Generic - any
Grammar, now that you can load facts into keywords, can capture them too.
``Token`` comes with its grammar's syntax (`'UNIQUE'`, in quotes), so the comparison.
The tree was felled without being cut down.

Controlled Test: An Index Cannot Win the Mark — "The Bank Guarantees"
Exactly the assertion that cannot be invented.

The only thing NOT made, and why

Column-level DML Investigation and Oversight: A Deeper Dive and Oversight Exclusion: The Version
Simple-minded produces the same class of error that this round existed to eliminate. Capture the
Column is trivial; solving it isn't. `resolveNamed` requires exactly one candidate.
They exist in dozens of tables—so almost all edges would fall.
In case of fallback, and a single node would aggregate all writes from ALL tables. "Who
He would respond with whom he writes the identical column.
apresentado como se fosse a dela.

The blockade was necessary; no longer is there token capture (the token's capture has become available in this round)
It is what resolves the uniqueness of the index), lacks QUALIFICATION. The table is a sister to the column
In the tree and the captures resolve downward, then a pattern that fits the column not
It reaches the table, and one that fits the statement only reaches the first column. The drawing that
resolve - inline 0 captures, inline 1, and declaration index
qualified by `context.name` — written in the backlog with the acceptance criteria that
Detects aggregation if it recurses.

## Fronteira reafirmada

O motor conhece FORMATOS (`xml`, `sql`, `json`), nunca FERRAMENTAS. Reconhecer as
The concrete flow orchestrator's structure is customized grammar of the project.
Consumer role here is to deliver the generic apparatus – and when the consumer doesn't
He can get there with him, the hole is here even if nothing is there.
tecnicamente quebrado.

## Progress Log

August 15, 2026 - Diagnosis, correction, graph tests, three repetitions cuts, three
  itens de backlog. `go test ./...` verde. Falta reindexar os consumidores: o cache de
Shards are keyed by content hashes and only the bump of `shardCacheVersion` invalidates it.
With the daemon running the new binary.
August 15, 2026 (same session, later) - Correction by Engineer: I had assigned one
Found the error in the query agent's reading, which had read the skill. The criterion becomes
The agent read the skill and missed = instruction is missing. Reclassified and redirected
under instruction: named labels by content (Phase 2 of the AST skill), diagnosis
Empty DML Query Prohibits Inference of Bank Guarantee from an INLINE_0 without Columns
And an inline error in the memory skill.
  `internal/memory/rule.go`.

Third Round: The Qualification, and the Item That Was Missing Entered

The column-level DML was out because the naive version aggregated. The lock was
QUALIFICAÇÃO, e ela agora existe como mecanismo do motor.

Generic - across both backends

Um alvo capturado passa a resolver como `QUALIFICADOR.NOME`, e `scan()` indexa toda
The declaration with `Context` also under `context + "." + name`. The field is the same in YAML and
Semantics follows the backend because the trees are different:

ANTLR: The pattern matches one node and captures them recursively, but the qualifier is a sibling (a)
The update table goes alongside the SET). The path is anchored in ANCESTRAL —
The first segment is the rule that you must follow. The chain has already existed in INLINE_0__;
It was missing INLINE_0 from accepting anchors, derived directly from its own queries.
Here is the translation:

"INLINE_0 and deliberately not turning INLINE_1, otherwise INLINE_2"

This translation maintains the structure of the original Portuguese text while rendering it in idiomatic English. The placeholders "`qualifierAnchors`", "`context_type`", and "`update_statement`" are left as they were to preserve the specific meaning intended by the original author.
  viraria dono de tudo dentro dele.
Tree-sitter patterns are structural and encompass the entire tree, so the qualifier is
  outra CAPTURA (`QualifierIdx` ao lado de NameIdx/ValueIdx/ParentIdx).

The decision that defines quality: a query asking for a qualifier but not getting one
It emits nothing. The unqualified boundary is not a lesser version of good; it is harmful.
Qualifying also makes the fallback honest: an inline 0 stub registers a column
de uma tabela, onde `ST_PROC` fundiria as de todas.

The type relationship whitelist has turned into an exclusion list.

`INLINE_0` was an array of Go relationship names - vocabulary in code.
Grammar stuck in the engine. It was stale in both directions: it admitted `CREATES`, `EXECUTES`, and
The following is an idiomatic English translation of the provided Portuguese text:

"`TRUNCATES`," which none of the grammars sent declares, and silently discarded any.
type new - extracted entities, cached references, no edges and no errors.
It turned into INLINE 0: the exclusion of what the engine routes on its own path.
(CALLS, INSTANTIATES, READS/WRITES_FIELD, INHERITS, IMPLEMENTS, IMPORTS, DECORATOR,
Export). What an engine possesses is closed-ended; what a grammar invents now
chega ao grafo sozinho.

Coverage in language, which is always the question to ask

`WRITES_COLUMN` declarado em **plsql, tsql, postgresql** (UPDATE + INSERT), **db2**
Only update, and **SQL/Tree-Sitter**. All trees are different, and each path came from
A dump is not of speculation. In DB2, INSERT does not descend into columns - declare the query
It would be an example that fits nothing at all, worse than being absent because it seems like cover-up.

Quarter Final: Closed Index Parity, and What It Reveals

The index form is now available in **PL/SQL, T-SQL, PostgreSQL, DB2, and SQL/Treesitter**.
Each path emerged from a dump. Two things came out with it:

The guard was unnecessary. As `ChildByRule` falls into the token, the capture
It returns empty when the mark is not there—so ONE query resolves both cases, and the
`INLINE_0` was simplified from two to one. In PostgreSQL, it's not even a token; it’s just the rule.
The `unique_` simply does not appear in a common index. Even the same field, the same answer.

``context_name_paths` was read only by the backend Tree-Sitter.` In ANTLR, the name of a variable is...
Context exited from INLINE_0 - field declared as name, otherwise first terminal.
It doesn't fail high: the first terminal of INLINE_0 is the word
Here is the translation:

"`CREATE`", then all entities within the statement would have a context called
"CREATE". Medido: em tsql, `create_or_alter_function`, `create_or_alter_procedure` e
`create_schema` respondiam "CREATE"; em postgres, `createtrigstmt` respondia a TABELA do
trigger em vez do nome dele. A mesma chave de YAML agora responde nos dois backends,
com o mesmo caminhador de regras.

**O que o teste-guarda deste repo desenterrou.** Declarar `context_types` em `sql.yaml`,
`tsql.yaml` e `postgresql.yaml` acionou `TestEveryCallableContainerIsDeclaredAsAContext`
And her sister, who demands that every declared container by a grammar be in
Here is the translation:

"Otherwise, parameters and columns are assigned to what surrounds them."
descartados. `sql.yaml` saiu de `flatLanguages` e passou a declarar `create_table`,
And beyond the index, which means parity was not just
Add an index — it was closing what was missing in these three grammars.

Quintuple Round: The backlog is attacked, and the repository switches to English.

### Idioma: decidido e aplicado

The Engineer closed the open question in the backlog - code and comments are
One hundred percent English. Translated approximately 520 lines: INLINE_0__ (100, the largest block and the...
The most valuable feature of module, `cache_convert.go`, is the AST tests and comments.
Of the 45 YAMls of grammar, a good part was just one paragraph repeated in 26.
Grammar rules - once translated and applied to all.

Two traps in the translation, both caught by test: a comment in `comment_entity_test`
It was a fixture, and his assertion mirrored the text (translated only one side broke the test);
Replacement of multiline block leaves an orphaned line when the block has changed since it was last used.
Written: The final character-by-character scan is what closes this.

Lint tool is enabled, and the backlog measurement was incorrect.

The backlog stated "zero issues" (0 issues). The measurement was conducted with
**Inline 1 does not overlay Inline 2 of **Inline 0**.
Brazilian Portuguese to idiomatic English:

The lint **never ran**. True, **25 dead symbols**, all
Removed (a third copy of `copyDirRecursive`, an integer `mockGit` that nothing)
builds an inline 0 of a benchmark that failed, inline 1,
In GitStore and other places, there is a green inline comment, and a dead test function
confirma que a rede pega.

Daemon: The current working directory is stable on startup.

At the beginning of `Daemon.Start`, the daemon inherited the current working directory from whom.
It emerged, including from a test that ran chdir for itself `t.TempDir()`.
After removing the directory, all tools that call `os.Getwd()` failed.
While those who resolved by `project_dir` were still functioning, that division was what did it.
The symptom appears like an incomplete module. Best effort for purpose: a daemon that cannot
Chdir is still running as a daemon.

### Avaliados e mantidos no backlog

Graph integrity sensor remains valid and valuable: detects mode
Silent BugDB Corruption String Test - None of the current tests catch it. It's work.
Clearly defined (_`graphit ast verify`), not here.
Two loose ends of the dream continue valid; the first (take it to the report)
The session warning that you didn't use the tool is cheap and should come first.
- **Flake do `TestMemoryGitStore_CreateOrphanBranch_Full`** — reproduzida uma vez nesta
The session was under load, passing three consecutive executions. The backlog diagnosis remains standing.
ICU in the bundle is blocked by the Engineer's decision ("return it, then I'll fix it later").
I resolve this) and requires verification on macOS and Windows, which cannot be done from here.
