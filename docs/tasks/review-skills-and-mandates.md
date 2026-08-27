Task: complete the review of skills and mandates

Status: Completed on July 28, 2026. Six stages — `f0432fef`, `70fa594c`, `39184383`, `09b73553`, __INLINE_4__.
Here is the translation:

"INLINE_0_" and "INLINE_1_" each with their own changelog in "INLINE_2". The first three were
The scope was originally intended; the last three came from corrections made by the Engineer during the work.
The below assessment serves as reference (commands, tool catalog, conventions); section
What remains at the end reflects what does not belong to this task.

---

## O que foi feito

Step 1 — `f0432fef` — each skill teaches its own module's tools

The _INLINE_0 was the significant gap that became the first call in every skill of the Hub, with semantics
documented (substring of ID/NAME/DescRIPTION, without stemming) because this changes how it is searched. The
Others who were absent entered all of them. The decisions that the survey left open:

They enter — not just lifecycle, but also the origins of contexts that
The rest of her skills were already being used without explanation from where they came.
And `memory_sync` and `memory_remove` enter together, in a section of imported contexts.
- **`knowledge_remove` entra por par** com install, com aviso: sem `context`, limpa o wiki local.
"He was outside of the lift registration" — he wasn't in the list and it's the narrow tool.
For the case where the watcher was unable to witness the change.

Also: `hub_type-path` was not on the own Hub's mandate, despite being used by the skill of
improvements; e o ecossistema de projetos (`cluster_*`) ganhou ensino de verdade em vez de uma
frase de passagem, a pedido do Engenheiro.

### Etapa 2 — `70fa594c` — `config`, `daemon` e `dream` sem sexto mandate

Selected Drawing: Sections from Existing Skills. Not for economy— the expensive is the mandate, which
It remains in context throughout the session, while the body of the skill is on-demand. What did you decide was...
Each domain already had a skill that led the agent to the door and left them without tools:

Domain | Skill | The trigger that already existed without mechanism
|---|---|---|
Brazilian Portuguese:
| `dream` | Improvements | "You noticed something out of the ordinary in this current change" - and there was no way to fix it. |
Here is the Brazilian Portuguese text translated into idiomatic English:

The inline comment says, "The daemon isn't running" — without how to check.
Brazilian Portuguese to idiomatic English:

| `config` | hub | The hub already owns `cluster_*`; configuration is the same slot |

This translation maintains the original meaning while using more natural phrasing in English.

The right question isn't about what domain the tool is for; it's **what the agent is doing when...**
She needs it. Precedent: The skills here are grouped by trigger, not prefix.

Step 3 — Review Content

The worst: The documentation was backwards and it overwrites **—** `DryRun`'s default `false`.
The called without parameter removes all candidates. The skill presented this as a dry run.

Also: the step INLINE_0 survives in the workflow contradicting the section that it belongs to.
commit `9e179bc9` acrescentou; o bloco obsoleto de sync ainda existia inteiro no ast; `hub_list`
received `project_dir` and an unnamed filter that doesn't exist, in the only mandatory step of the skill;
quatro lugares aceitavam ferramenta nativa sem o harness ter sido tentado; e `memory_search` era
described as inline read of `.md` when it is FTS5 on the compiled wiki.

Step 4 — Explore Another Project Begins in Ecosystem

Order given by the Engineer during the task. Mandatory when the question pertains to code or
Documentation outside this repository: `cluster_projects` First, follow the same tools MCP
with the `dir` of the brother as `project_dir`, and last, a native tool.

The missing distinction was: the skill of Ast treated imported context as the only way to consult.
Code Outside. The "sister project" registration already has its own graph, wiki, and memories - import it.
Reindexes what already exists and not knowing about it is what the agent was supposed to read file by file.
It turned into a four-line table (this repository, brother of the ecosystem, or unregistered checkout)
Dependence without check-in.

Nothing needs to be installed, linked, or imported: `project_dir` is a parameter. `hub_link` entered.
as an exception — brings an artifact for this project and does not grant access that passes
It's already done.

Step 5 — Inline 0 — Inline 1: Read page from Wikipedia via MCP

Request by the Engineer. Skills dictated "read the entity page," and reading the page was the task.
The only step without tools—there was reading directly from a file, which is exactly what fails.
When the agent is confined to their own workspace and the page belongs to another project.

`wiki_source` em MCP e CLI, com o mesmo fatiamento do `ast_source` (`head`, `tail`,
`start_line`/`end_line`, `line_numbers`, `pattern` + `regex`/`before`/`after`). `path` aceita slug,
Slug with either `.md` or relative path - filename for wiki title is generated without case differentiation
And the slug in hand rarely hits exactly.

The fatality was sent to INLINE_0 instead of getting a second copy: after searching for
Text in all instances of `ast.SourceService` is pure manipulation. `ErrPageNotFound` separates incorrect slugs from
Reference rejected, otherwise the list of alternatives suffices to bury the reason for rejection.

Step 6 — Inline 0 — Cipher, and the bug that wipes out live code

The Engineer observed that the agent used INLINE_0 and almost never queried. **It wasn't a lack of

Translation:

The Engineer noticed that the agent utilized INLINE_0 and rarely performed queries. It was not due to a deficiency.
Example — it was the header INLINE_0_. It became "the"
best way to FIND NAMES, never the answer"*, com a Fase 3 renomeada para *"where the question gets
answered"*.

And there's the worst find of all the revision: every callable appears twice in the graph.
Here is the Portuguese text translated into idiomatic English:

The ``CONTAINS`` sets up the connection to the ``File``; ``CALLS`` points to a key-stroked stub by name.

This translation maintains the technical structure and intent of the original Portuguese code snippet.
Empty and line `0`. We are different, so `NOT ()-[:CALLS]->(f)` is true for all.
declaration — `Apply` was reported as dead code with 13 callers, present in three locations, and one.
Agent follows any one of them and erases in use code.

The same cause: mixing types of edges around the same node returns **no lines and no errors**.
Two pre-existing queries always returned empty, hence.

---

What remains (does not belong to this task)

The comment at line `REFERENCES` is never persisted. The commit `6ab88223` states that each
comment loads an edge `REFERENCES` to the declaration that precedes it. Does not load:
`MATCH (c:Comment)-[:REFERENCES]->(t)` falha com *Table REFERENCES does not exist*. Causa raiz em
INLINE 0 type relationship is only registered when INLINE 1.
And since the comment adapter doesn't fill in INLINE_0, the table never gets created and writing is skipped.
Rejected. The nodes _INLINE_0_ and their edges, which can be reached by _INLINE_1_, are present; only the edge is missing. Task
separada aberta.

**`ast_index` via MCP grava no grafo do projeto errado.** ~~`openASTDBReadWrite` faz `chdir`, monta
The ``DefaultLadybugConfig()`` with ``DBPath`` relative, and returns a handle whose database only opens on
First query - when INLINE_0 has already reverted INLINE_1. ~~**Corrected**, along with the stub of~~
`DeleteRepository` — ver `docs/tasks/corrigir-indexacao-no-projeto-errado.md`. A causa raiz era mais
Extensive beyond what this paragraph suggested: the same defect appeared in four more places, including one.
`os.RemoveAll` de caminho relativo em `ast_index(reset: true)` que apagava o banco AST do projeto
errado.

The AST index of this project has 16 nodes of the probe, resulting from the above bug during verification.
Task: an _INLINE_0_ _INLINE_1_ that does not exist here, more entities and name comments
Invented. Nothing was destroyed—since the graph was empty and INLINE\_0 was a stub, so the call
Only added. They continue there: they exit with `ast_index(reset: true)`, or now that it works,
`reindex: true`.

**`__config__` tem `lang` vazio neste projeto.** A query *Identifying project frameworks* da skill
Returns the line with `frameworks` empty — the enrichment did not detect a framework in the CLI Go.
It's plausible. The query runs; it doesn't respond here.

``receiver_type` is narrower than it suggests.` The text refers to tracing calls.
Until the owning class, in this graph's sample, populated values were constructors.
JavaScript/TypeScript (inline 0, inline 1). Unmodified - the query runs and returns data, only covers less.
do que a frase promete.

---

## Onde as coisas ficam

What? Where?
|---|---|
| Template do mandate | `internal/hub/adapters/ide/mandate.go` → `ModuleMandateTrigger` |
Mandate + Skill of Each Module | `internal/{ast,hub,knowledge,memory,improvements}/rule.go` (Improvements also `rules.go`)
Content of Skill | Function INLINE_0 at the top of each INLINE_1 |
| Frontmatter da skill | dentro de `InstallSkill()`, string `"---\nname: …\ndescription: …\n---"` |
| Ferramentas MCP | `internal/mcpstdio/tools_*.go`, registradas via `brand.MCPToolName("dominio", "acao")` |
Reference to tool in text | INLINE 0 → renders with dashes

Tamanhos: `knowledge` 1000 linhas, `ast` 660, `improvements/rules.go` 566, `memory` 365,
`mandate.go` 364, `hub` 289.

Commands (not obvious — CGO, tags, and the Ladybug library)

```bash
export LBUG=~/go/pkg/mod/github.com/\!ladybug\!d\!b/go-ladybug@v0.17.0/lib
LD_LIBRARY_PATH="$LBUG:$LD_LIBRARY_PATH" go build -tags fts5 ./...
LD_LIBRARY_PATH="$LBUG:$LD_LIBRARY_PATH" go test -race -tags fts5 -p 4 -timeout 2400s \
  $(go list ./... | grep -v "/antlr/" | grep -v "/treesitter/")
golangci-lint run --timeout=5m     # RODE ANTES DE COMMITAR — a CI reprova por isto
make ci                            # vet, lint, vulncheck, test, ui, ui-lint
```

The approximately 26 warnings from the UI are already there and do not block.

---

Real Tool Catalog MCP (62 verified)

```
ast          ast_search ast_query ast_schema ast_source ast_list ast_index ast_export
             ast_embed ast_install ast_remove
hub          hub_search hub_show hub_list hub_install hub_link hub_unlink hub_update
             hub_submit hub_projects hub_uninstall hub_type-path
knowledge    knowledge_search knowledge_list knowledge_schema knowledge_lint
             knowledge_export knowledge_index knowledge_sync knowledge_install knowledge_remove
memory       memory_search memory_insert memory_update memory_list memory_important
             memory_promote memory_demote memory_delete memory_index memory_gc memory_schema
             memory_export memory_sync memory_remove
wiki         wiki_search wiki_browse wiki_xrefs wiki_log wiki_embed
cluster      cluster_get cluster_set cluster_unset cluster_projects
config       config_get config_set config_unset config_list
daemon       daemon_status daemon_stop
dream        dream_status dream_reports dream_subject_add dream_subject_list
             dream_subject_remove
improvements improvements_rules
```

The count was 62; there are 64 with `hub_type-path` (which was missing from the list), and not including those.
cinco de ciclo de vida (`init`, `sync`, `update`, `remove`, `version`).

## O levantamento original — lacunas, todas fechadas

The tools built into the module are missing from the skill's content.

Module Absentees
|---|---|
| ast | `ast_list`, `ast_index`, `ast_export`, `ast_embed` |
| hub | **`hub_search`**, `hub_submit`, `hub_projects`, `hub_uninstall` |
| knowledge | `knowledge_list`, `knowledge_schema`, `knowledge_lint`, `knowledge_export` |
| memory | `memory_export`, `memory_remove`, `memory_schema` |
| improvements | `improvements_rules` |

It is the most serious: The mandate instructs "to check the Hub through MCP before trusting in"
Own knowledge and skills never teach you how to use a search tool. The agent receives the order
sem o meio de cumpri-la.

The following is provided:

**Inline 0/Inline 1**, **Inline 2/Inline 3**, and **Inline 4** are of
Life cycle exception - decide if they enter, not assume that yes.

2. No domain without any skills

Before writing, decide on architecture: 

This phrase is idiomatic English translation of the given Portuguese text. It conveys that before starting to write or code, one should first determine and choose their architectural approach or design plan.
own skills for each, a skill "operations" covering three sections within the scope of
existentes.

Review of Content (most of it not yet started)

Read each line in full, looking for: obsolete instruction, example that doesn't work,
tool with an incorrect parameter, which the harness already automates, and location
where native tool is accepted without having been tried before with harness.

---

Done (do not re-do)

Commit `9e179bc9`:

Empty sections do not receive `ModuleMandateTrigger`, `triggers []string`, or `tools []string`.
  renderizam.
The five mandates rewritten with concrete triggers and inventory of tools.
- Bloco `⚡ MANDATORY: Sync After Every File Modification` do knowledge **removido**.
- Testes: `TestModuleMandateTriggerCarriesTriggersAndTools`,
  `TestModuleMandateTriggerOmitsEmptySections`.

---

Things Learned That Change Decisions

The mandate is the trigger, skill is instruction. The mandate says "when to open"; the procedure stays in place.
Skill. Do not move procedure to mandate.

Abstract mandate does not trigger. "For any structural analysis task, use MCP."
Policy, not trigger: who receives "thinks it's called 'saveUser'" doesn't classify that as
Analysis and structure, use grep. Write the trigger as it arrives in the request.

The watcher also reindexes the AST, not just the wiki. Confirmed in
`internal/daemon/syncmodule.go`: um watch, dois consumidores (`reindexAST` e `reindexKnowledge`),
Each with their own ignored file. And the memories compile themselves due to `MemorySyncModule` or
Sure, here is the Portuguese text translated into idiomatic English:

"Be it, `sync`, `ast_index`, `knowledge_sync` and `memory_index` are all exception tools."

This translation maintains the original meaning while making it sound more natural in English.

The tool that tightens wins big. When just one subsystem is wrong, **inline 0**,
**INLINE_0** or **INLINE_1** perform nearly half as much work as **INLINE_2**. **INLINE_3**
Indexes reindexes, updates both Wikis, memory, and the Hub.

**O daemon segura o write lock e a leitura falha com mensagem enganosa.** Uma consulta ao grafo
that fails on the reindexing retry window with `ladybug open: failed to open database with status 1` —
The name of the bank is read as "there is no graph here." It's locked; retrying works. Index:
verdade ausente diz outra coisa (`no AST database found at ...`). Documentado nas skills de ast e
Knowledge because falling into grep here is the most costly error available.

The watcher makes synchronization unnecessary. The daemon observes the tree of documents and reconstructs the wiki.
alone. Any instruction that suggests synchronizing after editing is obsolete — `sync` is
exception tool: stopped daemon, coming from outside the machine, or a proven index
old. What remains obligatory is **to write down** the registration, not to re-index it. Look for the same
Standard in INLINE_0 and INLINE_1.

**Nomear a ferramenta no mandate importa.** O agente decide entre MCP e nativa *antes* de abrir
Skill; until then, he only knows what was said by the one who gave the order.

Skills are created in Go, not Markdown. They're slices of strings concatenated —
`gofmt` depois de mexer, e cuidado com aspas escapadas.

Conventions of this repository

Code, comments, and names in **English**; commits, change logs, and documentation in **Portuguese**.
Changelog required at the end of each stage upon completion —
Atomic, not a giant changelog at the end.
- Nunca commitar automaticamente fora do fluxo pedido; nunca remover hooks do git.
- Nomes de sonda em teste devem ser **inventados**, nunca copiados do corpus real.
Nothing of mocking or stubbing in functional requirements without explicit authorization.

---

## Invariante que ficou no lugar do levantamento

A module test asserts that all tools it possesses are reachable from the ```
Own skill - because the mandate announces the inventory, and the tool announced that the skill does not
It teaches is order without means of fulfilling it. It was exactly the case with `hub_search`.

New MCP Tool, therefore, has two additional obligations beyond registration in `tools_*.go`: entering into
The test of the package
reprova se faltar a segunda.

The tests that verify **warning** — not just mention — exist because a single mention alone does not prevent it.
Error: omitting to say that the agent of the dream does not inherit the conversation produced
Useless subjects, and documenting INLINE_0 without saying that the naked call produces data loss.
