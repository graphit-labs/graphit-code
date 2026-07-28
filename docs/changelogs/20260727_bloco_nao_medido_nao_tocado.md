# Bloco "não medido / não tocado": travessia fundida, memória medida, sidecar endurecido, corrupção não reproduzida

**Data:** 2026-07-27
**Escopo:** `internal/ast/treesitter_adapter.go`, `internal/ast/astignore.go` (já commitado),
`internal/ast/antlr_sidecar.go`, `internal/daemon/syncmodule_*_test.go`, `docs/upstream/`
**Origem:** pedido do Engenheiro para resolver os quatro itens do bloco "Aberto — não medido /
não tocado"

---

## 1. Segunda travessia do tree-sitter — fundida

`extractDocstringsTS` era uma travessia completa da árvore executada **depois** do passe de
queries, que já havia encontrado toda entidade. Ela visitava cada nó para achar os poucos que
são declarações, e cada visita cruza para a biblioteca C várias vezes (filho, tipo, checagem
de nulo), então o custo acompanhava o tamanho do arquivo em vez da contagem de entidades.

Agora o passe de queries entrega os sítios: para cada nome capturado, `declSiteFor` sobe até a
declaração mais interna que a linguagem reconhece, e `attachDocstringsTS` examina só esses.

**A regra de pareamento não mudou** — mesma chave `(linha, nome)`. Declarações cujo nome fica
numa linha posterior à da declaração continuam sem documentação, como antes.

### Medição

O benchmark do componente diz 10,6×, e sozinho ele engana: o trabalho foi **realocado**, não
eliminado — subir do nome até a declaração custa saltos de pai, e esse custo agora vive dentro
do laço de queries, onde o benchmark do componente não o vê. O parse inteiro é o número
honesto:

| | tempo | memória | allocs |
|---|---|---|---|
| antes | 34,3 ms | 970 KB | 18.680 |
| depois | 31,8 ms | 732 KB | 11.183 |
| | **−7,3%** | **−25%** | **−40%** |

`BenchmarkParseGoFileEndToEnd` existe justamente para que a próxima pessoa não leia só o
componente.

### Dois defeitos pré-existentes que o teste novo expôs

`TestDocstringsSurviveTheRealQueryPipeline` passa pelo pipeline real, e não pelo harness
sintético do diferencial. Ele reprovou em dois casos — **ambos verificados como idênticos no
código anterior**, rodando o mesmo teste contra `HEAD`:

1. **Tipos Go nunca recebem seu comentário de documentação.** A query captura o nome do tipo,
   cuja declaração mais interna é `type_spec`, mas o comentário é irmão do `type_declaration`
   que a envolve. Nem a varredura antiga nem a coleta atual atravessam essa distância.
2. **Docstring de Python chega ao índice com `"""` no fim.** `cleanDocstring` remove a aspa
   tripla de abertura e não a de fechamento.

Os dois estão fixados no teste com o comportamento atual e comentário nomeando o defeito, para
não passarem por correto. **Não foram corrigidos**: este commit é de desempenho, e enfiar
mudança de comportamento nele esconderia as duas coisas.

## 2. Memória do `Entity.Source` — medida

O campo guardava `parent.Utf8Text(src)`, uma cópia em heap do corpo de cada entidade, só para
que `isExported` fizesse uma checagem de substring depois. Removido em `6aad6d2c`, nunca
medido.

Um benchmark de taxa de alocação **não enxerga** esse custo: `Utf8Text` continua sendo chamado
hoje — o veredito e a complexidade precisam do texto —, então os bytes são alocados de
qualquer jeito. O que mudou foi por quanto tempo ficam alcançáveis. A medida certa é heap vivo
após coleta forçada.

Sobre 40 arquivos Go deste repositório (379 KB de fonte, 10.199 entidades):

```
Entity.Source reteria:                     525 KB  (1,38x o tamanho do fonte)
heap vivo, texto descartado (atual):      2862 KB
heap vivo, texto retido (campo antigo):   3424 KB
diferença:                                 562 KB
pior arquivo isolado: parse_cache.go a 2,66x seu próprio tamanho
```

A primeira versão da medição usava `Properties` como veículo e inflava a diferença em 3,4 MB —
estava medindo o `map` por entidade, não o campo. O teste agora usa um struct com um `string`
simples, fiel ao que existia.

## 3. Sidecar ANTLR — endurecido

O sidecar existe porque as gramáticas ANTLR somam **47 MB de fonte gerado** (plsql 16 MB,
tsql 11 MB, db2 8,8 MB, postgresql 6,4 MB, cobol85 5,4 MB). Compilar tudo no binário o
inflaria, então cada gramática pode vir como executável separado, instalado do Hub em
`<projeto>/.graphit/grammars/antlr/antlr-sidecar-<lang>` e acionado por stdin/stdout — e tem
prioridade sobre o driver nativo quando presente.

Seus três testes existentes pulam sem `ANTLR_SIDECAR_BIN`, então numa rodada comum o driver
não tinha cobertura nenhuma — inclusive no tratamento de falha, que é onde estão os defeitos.
Foi escrito um sidecar de mentira que fala o mesmo protocolo e sabe se comportar mal de
maneiras específicas. Não depende de ANTLR e roda em qualquer lugar.

| defeito | estado |
|---|---|
| moldura de resposta de até 4 GB alocada a partir do cabeçalho | **reproduzido** e corrigido: limite de 256 MB e leitura que cresce com o que chega, via `io.CopyN` |
| processo morto devolvido ao pool ("put broken one back to avoid deadlock") | **não reproduzido** — as linhas só rodam se o *restart* também falhar. Corrigido mesmo assim |
| `Close()` retornava no primeiro slot vazio e vazava o resto | corrigido: drena todos, com espera curta por slot em uso |
| spawn ansioso no `initOnce` vazava processos quando uma partida falhava | corrigido: o pool nasce com slots vazios e o processo sobe sob demanda |

O pool agora guarda slots que podem estar vazios. Um slot vazio significa "sem processo, suba
um" — é isso que impede uma queda de custar um slot: o caminho de falha devolve o slot vazio
em vez do cadáver, e quem pegar depois tenta subir de novo.

**Não corrigido:** `cmd.Stderr = nil` continua mandando o diagnóstico do sidecar para
`/dev/null`. Capturar sem drenar arrisca bloquear, e a solução certa é um buffer circular —
fora do escopo deste commit.

## 4. Corrupção de string do Ladybug — terceira tentativa, não reproduzida

Não reproduzi. O que a sessão acrescentou foram **eliminações**, que é o que o report agora
carrega.

| hipótese | resultado |
|---|---|
| escrita concorrente | **não se aplica** — o motor recusa: `Only one write transaction at a time is allowed in the system`. O escritor de produção é serial, o que remove a classe inteira |
| coletor movendo/liberando a string Go por trás de um ponteiro C | `SetGCPercent(1)` + `runtime.GC()` contínuo, 3000 inserções em lote, leitor concorrente na mesma tabela: limpo |
| ponteiro Go passado ilegalmente para C pelo binding | mesma sonda recompilada com `GOEXPERIMENT=cgocheck2`: limpo, sem diagnóstico |

Sobra **escala junto com tamanho de valor**: o caso de campo eram 35358 linhas de arquivos
inteiros para 4 linhas ruins (~1 em 9000), e a maior sonda tem 3000 linhas sintéticas.

`docs/upstream/liblbug-string-corruption.md` é o 5º report, escrito explicitamente como
**não reproduzido** — observação de campo mais a tabela de eliminações. Um bom report pode
dizer o que o defeito *não* é, e para perda silenciosa de dado isso vale mais que mais um
palpite.

## Achado estrutural: os testes dependiam do runtime instalado

A suíte completa quebrou no meio da sessão, depois que o Engenheiro removeu `~/.graphit`.
Não foi regressão: `initTsExtMap()` monta a tabela de extensões no `init()` do pacote **só**
a partir dos arquivos de query do runtime instalado. Sem ele, `HasParserForExtension` responde
falso para tudo, e `classifyBatch` — que roteia por extensão — devolve vazio.

Consequências:

- **Um arquivo de query de projeto pode descrever uma linguagem que o parser depois se recusa
  a abrir**, porque queries de projeto não registram extensões; só o runtime registra.
- Checkout novo + `go test` dá falhas confusas em vez de pular.

`TestClassifyBatch` e `TestSyncModuleDoesNotTriggerItself` agora declaram a dependência com
`requireParsers`, que pula com a causa nomeada. `TestDocstringsSurviveTheRealQueryPipeline` e
os benchmarks novos contornam registrando a extensão à mão, o que os torna herméticos.

### Corrigido a pedido do Engenheiro: queries de projeto passam a registrar extensões

Um arquivo de query faz duas coisas — declara quais extensões atende e fornece os padrões de
extração — e as duas eram lidas de lugares diferentes. Os padrões passavam por
`resolveQueriesForLang`, que prefere a cópia do projeto. A declaração de extensão só era lida
por `initTsExtMap`/`initAntlrExtMap`, no `init()`, e **só do runtime instalado**. Resultado:
projeto sobrescrevia linguagem existente, mas não adicionava linguagem nova — a linha
`extensions:` num arquivo de linguagem nova era inerte, e o erro (`no grammar for .x`) parecia
gramática faltando.

O ANTLR já tinha o fallback de projeto em `AntlrParser.Parse`, mas ele era **inalcançável**:
`collectFiles` e o watcher filtram por extensão antes de qualquer parser ser chamado, e
perguntavam só à tabela global.

A resolução virou preguiçosa e em camadas, espelhando `resolveQueriesForLang`:

- `initTsExtMap` agora registra runtime **e** o diretório global do usuário (que também era
  ignorado), com o usuário por cima.
- `tsLangConfigFor(projectDir, ext)` consulta primeiro as queries do projeto, memoizadas por
  diretório em `projectTsExtCache` para não reconstruir o mapa a cada arquivo indexado.
- Novas `HasParserForExtensionIn`, `HasTreeSitterForExtensionIn`, `HasAntlrForExtensionIn` e
  `TreeSitterLangForExtensionIn`. As formas sem `In` continuam existindo e delegam com
  `projectDir` vazio, para os chamadores que não têm projeto (`server.go`, `obsidian.go`).
- Passaram a usar a forma ciente de projeto: `collectFiles` (descoberta), `Watcher.Start`,
  `CompositeParser.Parse`, `TreeSitterParser.Parse` e `classifyBatch` no daemon.

Coberto por `internal/ast/project_language_test.go`, que inventa uma linguagem existente só no
projeto e verifica as três camadas onde ela precisa aparecer: registro, descoberta e parse com
entidades e docstring.

### E recarga em runtime, porque o daemon vive dias

Registrar não bastava: os arquivos de query eram lidos **uma vez por processo**. Runtime e
usuário atrás de `sync.Once`, projeto atrás de um `sync.Map` sem invalidação. Instalar um pacote
de gramática ou editar um YAML à mão não tinha efeito até reiniciar — e não tinha em silêncio,
porque a descoberta simplesmente descartava os arquivos da linguagem nova.

Cada diretório passou a ser cacheado contra uma **assinatura** do seu conteúdo:

- `queryDirSignature` lê o diretório e soma nome, tamanho e mtime de cada `.yaml`. O mtime do
  diretório sozinho não serve: editar um arquivo no lugar não o move.
- A verificação é limitada a uma a cada `queryStaleCheckInterval` (2s), porque as consultas que
  ela alimenta rodam **uma vez por arquivo** — numa varredura de 35 mil arquivos o diretório é
  varrido um punhado de vezes, não 35 mil.
- Mudou a assinatura, recarrega e derruba o que foi derivado: `mergedQueryCache`,
  `compiledQueryCache`, `projectTsExtCache` e as tabelas de extensão.
- `InvalidateQueryCaches()` é o atalho para quem sabe que mudou algo. Ligado em
  `installGrammarArchive` e `uninstallGrammarFiles`, então instalar gramática vale na hora em
  vez de esperar o intervalo.

**As queries compiladas são descartadas, não fechadas.** Nada neste pacote nunca fechou um
`*sitter.Query` — eles vivem o processo inteiro. Um parse que já segura a fatia continua com
ponteiros válidos; fechar aqui seria use-after-free para ele. Vazar o punhado que um reload
órfã é o erro mais barato, e reload acontece quando alguém instala gramática, não em laço.

**As quatro tabelas globais viraram estado mutável compartilhado** e ganharam `extTablesMu`
(RWMutex, porque a leitura está no caminho quente por arquivo). O `init()` do ANTLR foi
removido: `rebuildExtTables` constrói os dois motores de uma varredura só, e um segundo `init`
rodaria antes ou depois do outro conforme a ordem dos arquivos, montando tabelas a partir de
fontes ainda não lidas.

Coberto por `internal/ast/query_reload_test.go`: arquivo adicionado depois do cache aquecido,
arquivo **editado no lugar** (que é o caso que a assinatura por mtime de diretório perderia), e
`InvalidateQueryCaches` valendo imediatamente. Rodam com `-race`.

### Gramáticas `.so`: reinício, não recarga

Arquivo de query recarrega no lugar; biblioteca de gramática não, e deliberadamente.
`resolveTreeSitterLang` memoiza cada linguagem pela vida do processo — resultado negativo
incluído — porque um `*sitter.Language` sustenta estado de parse vivo, e trocá-lo sob um parse
em andamento não é coisa que mutex resolva.

Sugestão do Engenheiro, e é a resposta certa: o daemon **já** sai para ser substituído quando
o carimbo do launcher muda (`versionTicker` → `stampChanged()` → `shutdown()` + `ErrReplace`).
Gramática nova passa a usar a mesma porta.

- `ast.GrammarSignature(projectDir)` resume as bibliotecas instaladas — nome, tamanho e mtime —
  varrendo `<projeto>/.graphit/grammars/{treesitter,antlr}` e o par global. Fica em
  `internal/ast` porque é o pacote que sabe onde os dois carregadores procuram; `GrammarDirsFor`
  evita que o chamador reafirme esses caminhos.
- O daemon guarda uma assinatura por diretório aceito (`grammarSigs`, sob `mu`), e o mesmo tick
  que checa o carimbo agora também compara essas.
- **Diretório visto pela primeira vez é registrado, não age.** Um projeto descoberto já com
  gramáticas instaladas não é motivo para reiniciar, e tratar como motivo faria o daemon
  quicar a cada projeto novo. Depois de registrado, uma instalação nele dispara normalmente.
- Remoção também conta: o processo ainda está segurando a biblioteca velha.

Coberto por `internal/daemon/grammar_restart_test.go`: instalação dispara e informa onde,
descoberta de projeto não dispara mas passa a vigiar, remoção dispara, e a assinatura é
atualizada ao disparar para não pedir substituição em laço.

## Estado

Suíte completa com `-race` limpa, sem `~/.graphit` presente.
