# Validação e2e do daemon: duas regressões do watcher, uma mascarando a outra

**Data:** 2026-07-27
**Escopo:** `internal/daemon/syncmodule.go`, `internal/daemon/syncmodule_e2e_test.go`,
`internal/daemon/syncmodule_classify_test.go`, `internal/daemon/syncmodule_feedback_test.go`
**Origem:** item 5 do handover — "validação e2e do daemon com o watcher novo", a única lacuna
onde código de produção nunca havia sido executado do jeito que roda de verdade

---

## Resumo

O e2e foi escrito, executado pela primeira vez, e **falhou**. Duas causas, ambas em
`handleBatch`, introduzido por `bd19121b` ("substitui todo polling de git por watcher de
filesystem"), de 26/07. A segunda estava escondida atrás da primeira: enquanto o classificador
estava quebrado, nenhum caminho chegava ao pipeline AST, e o loop de realimentação não tinha
como acontecer. Consertar a primeira tornou a segunda alcançável.

**A gravidade das duas é bem diferente, e vale registrar sem exagero:**

- O **defeito 1 estava vivo em produção** desde 26/07, em qualquer projeto, independente de
  configuração.
- O **defeito 2 não estava**, na trilha normal. `graphit init` injeta `.graphit/` no
  `.gitignore` (`cmd/graphit/commands/lifecycle.go:187`), e o watcher **obedece** o
  `.gitignore` — medido nos três cenários abaixo. Era alcançável só onde essa entrada não
  existe.

## Defeito 1 — o daemon parou de reindexar código

`handleBatch` classificava cada caminho como "documentação" ou "código" por localização:

```go
if isUnder(p, docsPath) { knowledgeTouched = true; continue }
```

`config.ResolveDocsDir` devolve `"."` quando `knowledge.docs_dir` não está configurado
(`internal/config/config.go:396`). Então `docsPath == projectDir`, e `isUnder` responde
verdadeiro para **todo arquivo do projeto**. `astChanged` nunca era populado; o `continue`
garantia que nada seguisse para o pipeline.

Efeito: desde `bd19121b`, para qualquer projeto que não configure `knowledge.docs_dir` para um
subdiretório, **o daemon nunca reindexou AST a partir de eventos de filesystem**. Só o caminho
`batch.Rescan` (eventos perdidos pelo kernel) continuava disparando varredura completa.

O poller anterior não tinha esse defeito porque não classificava nada: `reindex` chamava
`reindexAST` e `reindexKnowledge` incondicionalmente.

### Segundo problema no mesmo trecho

Os dois destinos não são exclusivos. `.md`, `.yaml`, `.json`, `.xml`, `.proto`, `.graphql` e
`.wsdl` são extensões de conhecimento **e** têm parser AST — medido, não suposto. E uma
varredura completa indexa `docs/guia.md` (verificado: gera entidades `heading` e `file`).
O `continue` fazia com que, mesmo com um `docs/` configurado corretamente, um `.md` sob ele
nunca fosse para o AST — incremental e completo discordavam sobre o conteúdo do índice.

### Correção

Classificação extraída para uma função pura `classifyBatch`, com as duas decisões
independentes:

- **AST**: extensão e nada mais, exatamente como a varredura completa decide.
- **Conhecimento**: sob o diretório de docs **e** com extensão que o wiki indexa. Localização
  sozinha não pode decidir, porque o padrão é a raiz do projeto.

## Defeito 2 — loop de realimentação ilimitado

Com o AST voltando a receber caminhos, o e2e passou a levar 5,1s por mudança em vez de 1,2s —
o teto de `syncMaxDebounce`, não o silêncio de `syncDebounce`. Sinal de que a árvore nunca
ficava quieta.

Uma sonda com uma única escrita externa e a árvore intocada depois:

```
batch 1:  1 changed,  0 removed — b.sql
batch 2:  5 changed, 14 removed — b.sql.edges.json
batch 3: 14 changed, 28 removed — manifest.json.nodes.json
batch 4: 25 changed, 32 removed — manifest.json.nodes.json.nodes.json
batch 5: 51 changed, 37 removed — manifest.json.edges.json.edges.json
batch 6: 99 changed, 76 removed — index.md.edges.json.nodes.json.edges.json
```

Não é só desperdício, é **amplificação**. O daemon escreve seus shards em `.graphit`, dentro da
árvore que ele observa. Um shard é `.json`, e `.json` tem parser. Indexar um shard emite um
shard do shard. Cada rodada produz mais arquivos que a anterior, sem limite — a sonda estourou
o timeout de 2 minutos ainda crescendo.

A varredura completa nunca viu esses arquivos porque a descoberta pula diretórios-ponto
(`internal/ast/writer.go:61`). O caminho escopado (`RunPipelineForPaths`) pula a descoberta
inteira — esse é o ganho da otimização — e perdeu a regra junto.

### Qual era a exposição real

A sonda acima rodou num `t.TempDir()` sem nenhum arquivo de ignore. Medindo o ignorer como ele
era construído antes desta mudança (`defaultPatterns` nil):

```
repo git, .graphit/ no .gitignore    → IsIgnored(".graphit", dir) = true
repo git, sem .gitignore             → false     ← o cenário da sonda
não é repo git, mas com .gitignore   → true
```

O watcher obedece o `.gitignore`, e `graphit init` injeta `.graphit/` nele. Em projeto
inicializado pela trilha normal o loop **não acontecia**. Ele exigia que a entrada estivesse
ausente — o que é possível, porque a injeção é best-effort: em `lifecycle.go:188` a falha vira
`StepWarn`, e em `internal/mcpstdio/tools_lifecycle.go:125` o erro é descartado com `_ =`.

Ou seja: era uma bomba armada, não uma bomba detonada.

### Correção — duas camadas

1. **O diretório da marca passa a ser excluído por padrão, no lado AST.** Não como literal
   inline no daemon, mas em `ast.DefaultAstIgnorePatterns`, consumido por
   `ast.NewAstIgnoreChecker` — espelhando o que o pacote `knowledge` já faz há tempo, onde
   `brand.DotDir() + "/"` está em `DefaultKnowledgeIgnorePatterns` desde sempre
   (`internal/knowledge/knowledgeignore.go:32`). A assimetria era essa: o lado do
   conhecimento se defendia sozinho, o do AST dependia do `.gitignore`.

   Isso cobriu **três** consumidores de uma vez, não um:

   | sítio | antes | exposto ao loop? |
   |---|---|---|
   | `internal/daemon/syncmodule.go` | `ignorer.New(…, nil)` | sim |
   | `internal/ast/watcher.go:49` (`graphit ast watch`) | `NewAstIgnoreChecker` → `nil` | **sim** — usa `RunPipelineForPaths` igual ao daemon |
   | `cmd/graphit/commands/runners.go:1709` | `ignorer.New(…, nil)` inline | sim |
   | `internal/ast/writer.go:46` (descoberta) | `NewAstIgnoreChecker` → `nil` | não — já pulava diretórios-ponto |
   | `internal/dream/dream.go:475` | `ignorer.New(…, nil)` | não — pula `brandDir` na mão |

   Os dois sítios que montavam `ignorer.New` inline passaram a usar o checker compartilhado,
   e o import de `ignorer` saiu dos dois arquivos.
2. **`classifyBatch` aplica a mesma regra da descoberta**: nenhum caminho com componente de
   diretório iniciado por ponto é fonte. Isto vale além do `.graphit` e é a parte com valor
   independente: a descoberta completa pula **todo** diretório-ponto
   (`internal/ast/writer.go:61`), mas o ignorer do AST é construído sem `defaultPatterns`, ao
   contrário do de knowledge. Então `.venv`, `.idea` e `.cache` só eram pulados no incremental
   se estivessem no `.gitignore` — full e incremental divergiam ali com ou sem `.graphit`.
   Só componentes de *diretório*: a descoberta pula diretórios-ponto, não arquivos-ponto,
   então um `.hidden.sql` na raiz continua sendo fonte.

Depois da correção o e2e caiu de 5,1s para **1,2s** por mudança — o debounce natural de 1s.

## Ajuste no próprio e2e

O teste afirmava que um arquivo criado **antes** do daemon subir apareceria no índice. O
daemon nunca prometeu isso: nada varre um projeto quando ele é adotado (`reconcileProjects`
apenas inicia módulos), e o índice é semeado por `ast index`. O teste agora semeia o índice
antes de subir o módulo, como produção faz — o que torna as asserções de "o resto do índice
continua lá" significativas em vez de vacuamente verdadeiras.

## Deriva de comportamento que fica para o Engenheiro decidir

O poller antigo rodava `RunPipeline` **completo** a cada mudança detectada, então pegava de
carona qualquer arquivo alterado enquanto o daemon estava parado. O watcher é estritamente
incremental a partir do momento em que sobe. **Mudanças feitas com o daemon desligado não são
mais recuperadas.** É consequência direta da otimização de escopo, não um bug — mas é uma
mudança de comportamento não documentada, e a decisão de aceitá-la ou de fazer uma varredura
na adoção do projeto é sua.

## Testes

- `TestAstIgnoreCheckerExcludesBrandDirByDefault` e
  `TestAstIgnoreCheckerStillReadsProjectIgnoreFiles` — o padrão vale sem `.gitignore` nenhum,
  não engole fonte de verdade, e não substitui o `.astignore` do projeto.
- `TestClassifyBatch` — 10 casos, unitário e rápido, cobre as duas regras e as duas regressões.
- `TestSyncModuleDoesNotTriggerItself` — uma escrita externa, exatamente um batch; verifica
  também que a mudança *chegou* ao índice (um watcher mudo passaria numa checagem de "sem
  loop") e que nenhum shard-de-shard existe.
- `TestSyncModuleEndToEnd` — criar, editar, apagar e escrever arquivo sem parser, através do
  `Start` público, com watcher e debounce reais.

Suíte completa com `-race` limpa.
