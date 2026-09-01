---
title: Instruções geradas em drift — preamble do mandate não propaga e os templates de query da skill AST são recusados pelo planner
status: done
created: 2026-09-01
updated: 2026-09-01
tags: [mandate, agents-md, skill, ast, canonical-catalog, cypher, generated-text]
---

# Instruções geradas em drift — preamble do mandate não propaga e os templates de query da skill AST são recusados pelo planner

## Objective

Dois defeitos independentes, ambos na categoria "texto gerado que instrui agentes está
errado e ninguém percebe", descobertos a partir de um sintoma único relatado pelo usuário:
`graphit sync` não atualiza o `AGENTS.md` com mudanças no `mandatePreamble`, mesmo com o
binário instalado atualizado.

**Defeito 1 — o preamble nunca é comparado.** `UpsertMandateTrigger`
(`internal/hub/adapters/ide/mandate.go`) decide se reescreve o arquivo comparando apenas o
`triggerContent` de cada módulo. O `mandatePreamble()` entra na montagem do bloco mas não
entra em nenhuma comparação de frescor. Consequência: mudar só o preamble nunca dispara
reescrita, e o `AGENTS.md` fica com o texto antigo indefinidamente.

**Defeito 2 — a skill `graphit-ast` ensina queries que o próprio motor recusa.** O texto é
gerado por `internal/ast/rule.go`. Ele documenta a recusa `canonical catalog: ...` numa
seção (linhas 387-406) e depois entrega ~96 templates de traversal, dos quais a grande
maioria viola as regras que ele acabou de enunciar. A query que eu mesmo rodei nesta sessão
e tomou erro veio do bloco marcado `🔒 MANDATORY: Before You Edit` (linha 83).

**Defeito 3, descoberto durante a investigação — `ORDER BY` está quebrado no planner.**
`canonicalTraversalPattern` tenta aceitar um tail `ORDER BY`/`LIMIT` mas não consome os
argumentos, então `RETURN DISTINCT e.name ORDER BY e.name` cai na checagem de projeção e é
recusado. Existe um comentário órfão em `ladybug_icebug_canonical.go:32` descrevendo um
`canonicalTraversalTail` que não existe no código, e nenhum teste cobre `ORDER BY`.

O objetivo é: corrigir os três, e deixar um teste que impeça o texto gerado de voltar a
divergir do motor.

## Raciocínio e justificativa da abordagem

O que amarra os três defeitos é a mesma classe de risco: **instrução gerada que ninguém
executa**. O preamble é texto que o agente lê; os templates são texto que o agente copia. Nos
dois casos o erro é invisível para quem escreveu, porque o artefato só é exercitado por outro
agente, depois, em outra sessão.

Por isso a correção central não é reescrever os exemplos — é **fechar o loop de validação**:

- Para os templates, a validação aceitável é executar cada um no MCP. O usuário corrigiu
  isso explicitamente nesta sessão, e a correção está memorizada
  (`01M1DGYP5JG0ZNYSVYHDB1M5RK`): ler o parser valida contra o source tree, o MCP responde
  pelo binário instalado e pelo grafo real, e os dois divergem.
- Como complemento (não substituto), um teste unitário: `internal/ast/rule.go` e
  `internal/ast/ladybug_icebug_canonical.go` estão **no mesmo package `ast`**, então o teste
  pode extrair os blocos de query do texto gerado e passar cada um por
  `parseCanonicalTraversal` — o próprio planner validando a documentação do planner.
- Para o preamble, o mesmo princípio: o hash cache passa a cobrir o preamble, então a
  propagação deixa de depender de coincidência.

### O contrato real, estabelecido por execução no MCP

A mensagem de erro do planner entrega o contrato inteiro. Para os **8 tipos lógicos** —
`CALLS`, `CONTAINS`, `HAS_FIELD`, `HAS_PARAMETER`, `IMPORTS`, `READS_FIELD`, `REFERENCES`,
`WRITES_FIELD` — a **única** forma aceita é:

```
MATCH (a)-[:TYPE]->(b) [WHERE ...] RETURN DISTINCT b.property [, b.prop2 ...]
MATCH (a)-[:TYPE]->(b) [WHERE ...] RETURN count([DISTINCT] b.uid)
```

com exatamente uma ponta filtrada. E o planner é o **único** caminho para esses tipos —
`WITH`, `UNION`, múltiplos `MATCH` não escapam, eles são recusados com a mensagem que enumera
a forma acima.

Formas provadas no MCP nesta sessão (todas retornaram sem erro):

| Forma | Status |
|---|---|
| `MATCH (caller)-[:CALLS]->(t:Function {name:'X'}) RETURN DISTINCT caller.name, caller.path` | ✅ |
| `MATCH (f:Function {name:'X'})-[:CALLS]->(callee) RETURN DISTINCT callee.name, callee.path` | ✅ |
| `MATCH (caller)-[:CALLS*1..3]->(t:Function {name:'X'}) WHERE toLower(caller.name) CONTAINS 'test' RETURN DISTINCT caller.name, caller.path` | ✅ |
| `MATCH (f:File {path:'X'})-[:CONTAINS]->(e:Function) RETURN DISTINCT e.name, e.line_number` | ✅ |
| `MATCH (f:File)-[:IMPORTS]->(m:Module {name:'os'}) RETURN DISTINCT f.path` | ✅ |
| `... RETURN count(caller.uid)` / `count(DISTINCT caller.uid)` | ✅ |

Recusadas no MCP (e presentes hoje na skill como exemplo a seguir):

| Forma | Recusa |
|---|---|
| `RETURN caller.name, label(caller) AS type, caller.path` | `a label is not projectable here` |
| `RETURN count(caller) AS callers` | `must be DISTINCT` — só `count(caller.uid)` funciona |
| `RETURN f.name, f.path, count(caller) AS callers ORDER BY ...` | `must project exactly one end` |
| `RETURN m.name, count(f) AS imported_by ORDER BY ...` | `must project exactly one end` |
| `RETURN e.name, e.line_number` (sem DISTINCT) | `must be DISTINCT` |
| `RETURN DISTINCT e.name ORDER BY e.name` | `not a plain property projection` — defeito 3 |
| `MATCH ... WITH f, count(caller) ... RETURN ...` | `must be a single MATCH ... the planner is the only route` |

E um achado à parte: **`IMPLEMENTS` não existe no grafo deste projeto.** A skill ensina
`MATCH (c:Class)-[:IMPLEMENTS]->(i:Interface ...)` afirmando que cobre interfaces de Go; a
tabela não existe e o erro lista os labels presentes. `INHERITS` idem, a confirmar.

### Alternativas consideradas e descartadas

- **Só corrigir o template da linha 83** (o que me deu erro): descartado. O defeito é
  sistêmico, 28 templates projetam `label()` e 87 omitem `DISTINCT`; corrigir um deixa os
  outros ensinando errado.
- **Escrever o lint só como regex sobre o texto**: descartado. Reimplementaria as regras do
  planner num segundo lugar, que é exatamente o modo de falha que criou este bug. Chamar
  `parseCanonicalTraversal` é a fonte única.
- **Não mexer no `ORDER BY`** e reescrever os templates sem ordenação: descartado. O motor
  claramente pretende aceitar o tail, o comentário órfão prova a intenção, e sem `ORDER BY`
  metade das perguntas úteis ("os mais complexos", "os mais chamados") ficam sem forma. Mas
  é mudança no motor de query, então vai isolada, com teste, e sinalizada ao usuário.

## Achados que só a execução no MCP revelou

Três defeitos **silenciosos** — nenhum deles dá erro, todos devolvem resposta plausível e
errada. Foram encontrados porque o usuário exigiu execução no MCP em vez de leitura do
parser, e o lint unitário aprova os três.

**1. Traversal sem tipo (`-[r]->`) devolve lixo, não erro.** Este era o template do bloco
`🔒 MANDATORY: Before You Edit`. Executado contra `UpsertMandateTrigger`, que tem 6 callers
reais, devolveu 25 linhas e **nenhum caller**: o pattern casou com as tabelas físicas
*reverse*, então `dependent` veio como `Field`/`Parameter`, `type(r)` vazou nome interno
(`reads_field__function_field_reverse`) e `path` veio absoluto. Um refuso é visível; isto
não é. Por isso o lint ganhou uma regra própria e a skill ganhou um aviso 🔒.

**2. Reindex incremental cria nó `File` duplicado com path ABSOLUTO.** Depois de eu editar
`mandate.go`, o grafo passou a ter dois nós `File` para ele: `internal/hub/.../mandate.go` e
`~/.../mandate.go`. Entidades novas também vieram com `path` absoluto sem a
barra inicial (`home/lainosantos/...`), contradizendo a skill, que promete path sempre
relativo à raiz.

**3. Consequência do 2: traversal ancorada em `File {path: ...}` devolve os filhos do arquivo
ERRADO.** Enquanto o duplicado existia, `MATCH (f:File {path: 'internal/hub/adapters/ide/mandate.go'})-[:CONTAINS]->(s:Import)`
devolveu os imports do `mandate_resume_test.go`, e o mesmo para `Comment`. A query node-only
equivalente (`MATCH (s:Import) WHERE s.path = '...'`) devolvia o certo, o que isolou o
problema no lado da âncora. **`graphit_sync` corrigiu**: o duplicado desapareceu e a traversal
passou a devolver os 10 imports e os 60 comentários corretos.

> Registro honesto do meu próprio quase-erro: eu estava a um passo de reportar o item 3 como
> bug de join do planner canônico. Era índice sujo. O que separou as duas hipóteses foi rodar
> `graphit_sync` e repetir a query — exatamente o procedimento que o mandate descreve e que eu
> ia pular.

## Plan & Task Breakdown

- [x] **T1 — Abrir o task log** — Spec: este arquivo, com objetivo, contrato provado no MCP e
  plano. Aberto antes da primeira edição de código.
- [x] **T2 — `mandatePreamble` entra na decisão de reescrita** — Spec: toca
  `internal/hub/adapters/ide/mandate.go`. Adicionar `Preamble` ao `mandateHashCache`,
  bloquear os três fast paths de `UpsertMandateTrigger` quando o hash do preamble divergir, e
  gravar o hash em toda escrita. Feito = mudar só o `mandatePreamble()` e ver o `AGENTS.md`
  ser reescrito. Invariante: idempotência preservada — duas syncs sem mudança não podem
  alterar o arquivo (`TestUpsertMandateTrigger_Idempotent`).
- [x] **T3 — `RemoveMandateTrigger` regenera o preamble** — Spec: mesmo arquivo. Hoje ele
  reaproveita o `inner` antigo literalmente, preservando preamble velho — foi provavelmente o
  caminho que escreveu o `AGENTS.md` atual ao remover o módulo `improvements` (commit
  `bc357e7`). Passar a remontar com `mandatePreamble()` + `assembleTriggers`, e remover a tag
  do cache. Feito = `imp_rule` deixa de existir em `mandate.hash`.
- [x] **T4 — Corrigir o tail `ORDER BY`/`LIMIT` do planner** — Spec: toca
  `internal/ast/ladybug_icebug_canonical.go`, `canonicalTraversalPattern`. O tail precisa
  consumir os argumentos do `ORDER BY`/`LIMIT`, não só as palavras. Remover o comentário
  órfão do `canonicalTraversalTail`. Feito = `RETURN DISTINCT e.name ORDER BY e.line_number`
  responde no MCP. Invariante: não afrouxar nenhuma outra regra — as recusas provadas acima
  seguem recusando.
- [x] **T5 — Reescrever os templates do `rule.go` para a forma única** — Spec: toca
  `internal/ast/rule.go`. Cada traversal sobre os 8 tipos lógicos vira
  `RETURN DISTINCT reached.prop` ou `count(reached.uid)`; nada de `label()` na projeção; nada
  de projetar as duas pontas; agregação/ranking sobre traversal sai ou vira duas queries.
  Corrigir a linha 83 (Pre-Edit Impact Check) primeiro, por ser a marcada obrigatória.
  Feito = todo template executado no MCP sem erro.
- [x] **T6 — Corrigir o escopo do parágrafo do canonical catalog** — Spec: `rule.go:389`.
  Hoje diz "a mounted/remote graph — a Hub context, an imported bundle", o que dá permissão
  implícita para ignorar a regra no grafo local. O grafo local é servido pelo mesmo planner
  (provado: recusa sem `context`). Reescrever enunciando a forma única como o caminho normal.
- [x] **T7 — Verificar `IMPLEMENTS`/`INHERITS`** — Spec: confirmar no MCP quais desses tipos
  existem. Ajustar ou marcar como dependente de linguagem os templates que os usam, incluindo
  a afirmação de que Go produz `IMPLEMENTS`.
- [x] **T8 — Teste de lint do texto gerado** — Spec: novo teste em `internal/ast`. Extrai
  todo bloco de query do texto gerado por `rule.go` e passa por `parseCanonicalTraversal`;
  falha nomeando o template e a recusa. Feito = o teste pega hoje os 28+87 e passa depois de
  T5.
- [ ] **T9 — `make install` e `graphit sync`, na ordem** — Spec: builder mudou, então
  reinstalar ANTES de regenerar, senão o daemon reescreve com a versão instalada velha
  (memória `01KZWFC40QFEP8TDCVBV3MT51Z`). Feito = `AGENTS.md` com o preamble novo e os
  `SKILL.md` com os templates corrigidos.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `docs/tasks/generated-instructions-drift-preamble-and-canonical-query-templates.md` | Created | este log |
| `internal/hub/adapters/ide/mandate.go` | Modified (T2, T3) | preamble entra na decisão de reescrita; remoção regenera |
| `internal/ast/ladybug_icebug_canonical.go` | Modified (T4) | recusa explícita de `ORDER BY`/`LIMIT`; comentário órfão removido |
| `internal/ast/rule.go` | Modified (T5, T6, T7) | 67 templates corrigidos, escopo da recusa reescrito, forma única documentada |
| `internal/ast/rule_query_templates_test.go` | Created (T8) | lint do texto gerado contra o planner real + regra de traversal sem tipo |
| `internal/hub/adapters/ide/mandate_preamble_propagation_test.go` | Created (T2, T3) | três testes de propagação do preamble |

## Technical Debt

- [ ] O `mandate.hash` em `.graphit/runtime/` carrega `imp_rule`, tag de um módulo já
  removido. T3 deve limpar; se não limpar, fica lixo inofensivo mas confuso.
- [ ] Os `rule.go` de `knowledge`, `memory` e `hub` não foram auditados. Se algum ensina
  query, está sujeito ao mesmo defeito e ao mesmo lint de T8.
- [ ] A skill afirma `count`, `sum` e `ORDER BY` funcionam em traversal ("You are not limited
  to lookups"). Com o contrato real, `sum` e agregação com projeção das duas pontas não
  funcionam. A afirmação precisa ser reescrita, não só os exemplos.

## System Knowledge

- **`UpsertMandateTrigger` tem três fast paths**, e nenhum olha o preamble: (1) hash do
  trigger bate e mtime do arquivo bate → retorna sem ler o arquivo; (2) hash bate, mtime não
  → lê, reparseia o trigger, confere ordem canônica → retorna; (3) sem cache → compara
  `m[1] == triggerContent` + ordem canônica → retorna. Zerar o `mandate.hash` **não** resolve,
  porque o caminho (3) também ignora o preamble.
- **`RemoveMandateTrigger` preserva o preamble antigo literalmente** — ele opera sobre o
  `inner` lido do arquivo, tira a tag e reescreve. É um segundo caminho de escrita que
  produz `AGENTS.md` com preamble velho, e explica o mtime `00:31:25` do arquivo atual ser
  posterior ao `make install` das `00:28:41` sem que o texto tenha atualizado.
- **Os 8 tipos lógicos têm caminho único.** Não é só uma restrição de projeção: `WITH`,
  `UNION` e múltiplos `MATCH` sobre esses tipos são recusados. O planner é a única rota,
  então "escrever de outro jeito" não é uma saída — a pergunta tem que caber na forma.
- **`count(caller)` não é `count(caller.uid)`.** `canonicalCountPattern` exige `.uid`
  explícito; `count(var)` cai na regra do `DISTINCT` e é recusado.
- **`label()` é proibido na projeção mas permitido no `WHERE`** de um traversal — a checagem
  é sobre a return clause. `MATCH (caller)-[:CALLS]->(t) WHERE (label(t) = 'Function' OR ...)`
  passa.
- **Query só de nós não é afetada por nada disso.** `label(n)` funciona, `ORDER BY` funciona,
  agregação funciona. A restrição inteira é sobre atravessar um tipo lógico.
- **`internal/ast/rule.go` e o planner são o mesmo package `ast`** — o que torna possível
  validar o texto da skill chamando o parser real.

## Progress Log

### 2026-09-01

- Diagnóstico do sintoma relatado: `graphit sync` não propaga `mandatePreamble`. Causa
  isolada em `UpsertMandateTrigger` — o hash cobre só `triggerContent`. Confirmado com o
  conteúdo de `.graphit/runtime/mandate.hash` (chaves `hashes`/`mtimes`, nenhuma do preamble)
  e com o diff staged de `mandate.go`, que mostra a frase removida do preamble ainda presente
  no `AGENTS.md:12`.
- Confirmado que o binário instalado (`00:28:41`) é mais novo que a edição do fonte
  (`00:21:28`), então "o bin está atualizado" procede e não explica o sintoma.
- `TestMandatePreambleReAppliesAfterAnInterruption` está falhando: a edição staged removeu a
  frase `re-open the skill for the domain you are re-entering` que o teste exige. Precisa ser
  decidido junto com T2 se a frase volta ou se o teste muda — é edição do usuário, não minha.
- Pergunta do usuário sobre a skill: confirmado que `internal/ast/rule.go` documenta a recusa
  em 387-406 e a contradiz em 28 templates que projetam `label()` e 87 que omitem `DISTINCT`,
  de 96 traversals com `RETURN`. O template que eu executei e tomou erro é o da linha 83, no
  bloco `🔒 MANDATORY: Before You Edit`.
- Correção do usuário: validar template executando no MCP, não lendo o parser. Memorizada
  (`01M1DGYP5JG0ZNYSVYHDB1M5RK`) e adotada — o contrato acima foi todo estabelecido por
  execução.
- Descoberto o defeito 3 (`ORDER BY` quebrado) e que `IMPLEMENTS` não existe neste grafo.
- Próximo: T2.

### 2026-09-01 — implementação

- **T2/T3 fechados.** `mandateHashCache` ganhou o campo `Preamble` (SHA256 do
  `mandatePreamble()`), e os três fast paths de `UpsertMandateTrigger` passaram a exigir que ele
  bata. O caminho que retorna sem ler o arquivo é coberto pelo hash no cache; os dois que leem
  são cobertos também por `fileCarriesCurrentPreamble`. `RemoveMandateTrigger` deixou de
  reaproveitar o `inner` e agora remonta com `mandatePreamble()` + `assembleTriggers`, além de
  limpar a tag removida do cache. Três testes novos, incluindo o caso do cache antigo com
  `Preamble` vazio, que não pode ser lido como "bate".
  `TestUpsertMandateTrigger_Idempotent` segue passando — duas syncs sem mudança não tocam o
  arquivo.
- **T4 fechado, mas NÃO como planejado.** A intenção era consumir o tail `ORDER BY`/`LIMIT` no
  `canonicalTraversalPattern`. Abandonado ao descobrir que **não existe estágio de ordenação
  nenhum** no planner — as duas únicas ordenações do arquivo são de determinismo interno
  (`sort.Slice` por key e por Rows). Consumir o tail devolveria resultado sem ordem, em
  silêncio, sob uma cláusula que pede ordem — pior que recusar. Então a recusa passou a ser
  explícita, com mensagem que nomeia o motivo e diz que query node-only aceita `ORDER BY`
  normalmente. O comentário órfão do `canonicalTraversalTail` (identificador que nunca existiu)
  foi removido.
- **T5/T6/T7 fechados.** 67 templates recusados → 0, e 11 traversals sem tipo → 0, sobre 137
  exemplos. As correções não foram só mecânicas: três famílias de pergunta **não são
  expressáveis** e foram substituídas por texto que diz isso em vez de exemplo que falha —
  dead code varrendo o grafo, ranking por fan-in/fan-out, e ranking de módulos mais
  importados. Todas as três precisam de traversal que não filtra nenhuma ponta ou que projeta
  as duas. Também documentado que propriedade de aresta (`receiver_type`, `source_file`,
  `full_call_name`) é inalcançável: não é projetável e predicado sobre `r` é recusado — o que
  torna a tabela de propriedades de relacionamento da skill enganosa por si só.
- **T8 fechado.** `TestSkillQueryTemplatesAreAcceptedByTheCanonicalPlanner` extrai os exemplos
  do texto gerado (blocos cercados, células de tabela e prosa, desembrulhando backticks) e
  passa cada um por `parseCanonicalTraversal` — o planner validando a documentação do planner,
  possível porque `rule.go` e o planner são o mesmo package `ast`.
  `TestSkillQueryTemplatesAlwaysNameTheRelationshipType` cobre a classe que o planner não
  pode pegar. Um teste que já existia, `TestASTRuleContentRunnableQueriesUseRealProperties`,
  pegou meu próprio template esquemático abstrato — reescrito com placeholder `reached.<prop>`.
- **Validação no MCP:** 90 traversals únicos extraídos; as formas distintas foram executadas
  contra o grafo real. Provadas: projeção de 1–3 propriedades da ponta alcançada,
  `count(x.uid)` e `count(DISTINCT x.uid)` com e sem alias, `*1..3` com e sem `WHERE`, âncora
  por `{prop}` e por `STARTS WITH`, e `CALLS`/`CONTAINS`/`IMPORTS`/`HAS_FIELD`/`HAS_PARAMETER`/
  `READS_FIELD`/`WRITES_FIELD`. Os templates que usam `IMPLEMENTS`, `INHERITS` e o conjunto
  DML/DDL não são executáveis aqui porque **este grafo não tem essas tabelas** — daí o aviso
  novo mandando chamar o schema antes de confiar nelas.
- **`go build ./...` limpo; `internal/ast` e `internal/hub/adapters/ide` verdes.**
- A frase que o diff staged do usuário removia (`re-open the skill for the domain you are
  re-entering`) reapareceu no working tree durante a sessão, então
  `TestMandatePreambleReAppliesAfterAnInterruption` voltou a passar. O índice ainda carrega a
  remoção staged: divergência index/worktree numa linha, deixada para o usuário decidir. Eu não
  editei essa frase em nenhuma direção.
- Próximo: T9 (`make install` rodando em background), depois `graphit sync` e conferir o diff
  do `AGENTS.md` e dos `SKILL.md`.

## Technical Debt (atualizado)

- [ ] **Path absoluto em reindex incremental.** Arquivo recém-editado entra no grafo com `path`
  absoluto e cria nó `File` duplicado, e enquanto o duplicado existe uma traversal ancorada em
  `File {path: ...}` devolve os filhos de outro arquivo. `graphit_sync` limpa. É bug real de
  normalização, não coberto por este task log, e o modo de falha é silencioso — merece task
  própria.
- [ ] **Sem estágio de ordenação no planner canônico.** `ORDER BY`/`LIMIT` sobre traversal
  agora é recusado com clareza, mas implementar ordenação sobre o set materializado é a
  correção de verdade e devolveria várias perguntas úteis à skill.
- [ ] **Ranking e varredura agregada indisponíveis** nos 8 tipos lógicos. Enquanto for assim, a
  skill não pode oferecer detecção de dead code nem ranking de acoplamento — hoje ela diz isso,
  o que é honesto, mas é capacidade perdida.
- [ ] **`rule.go` de `knowledge`, `memory` e `hub` não auditados** contra o mesmo lint.
- [x] `imp_rule` órfão no `mandate.hash` — `RemoveMandateTrigger` agora limpa a tag do cache.

### 2026-09-01 — T9 e verificação fim a fim

- `make install` concluído (inclui `npm run build` da UI), launcher re-extraiu sozinho: os dois
  stamps (`~/.graphit/runtime/dev/.build-id` e `~/.graphit/daemon/launcher.stamp`) foram
  atualizados às 01:10:11, e o daemon reiniciou.
- `graphit sync` regenerou os cinco `SKILL.md` do módulo ast (`.agents`, `.claude`, `.codex`,
  `.kiro`, `.opencode`). Conferido no artefato gerado: zero `label()` projetado sobre traversal,
  zero `count(caller)` sem `.uid`, e os quatro `-[r]->` que restam são os AVISOS contra ele, não
  templates.
- `.graphit/runtime/mandate.hash` agora carrega o campo `preamble`
  (`0f5f5d70b9337691...`), que nunca existiu, e `imp_rule` desapareceu das chaves — as duas
  mudanças de estado observáveis de T2 e T3.
- **Prova fim a fim do bug original**, porque um sync sem diff não prova nada: plantei o sintoma
  exato relatado — texto de preamble antigo no `AGENTS.md` com binário novo instalado —
  substituindo a primeira frase da seção AN INTERRUPTION por `TEXTO ANTIGO PLANTADO: ...`. O
  `graphit sync` seguinte detectou e reescreveu: o texto plantado saiu (0 ocorrências), o
  preamble correto voltou (1), e os quatro módulos sobreviveram (8 tags, abre + fecha). Antes
  desta correção aquele texto sobreviveria indefinidamente, que é precisamente o relato do
  usuário.
- Suíte final: `go build ./...` limpo, `internal/ast` e `internal/hub/adapters/ide` verdes,
  `gofmt` limpo.
- Scaffold `internal/ast/dump_skill_queries_test.go` removido depois de servir à validação no
  MCP; o lint permanente cobre a necessidade durável.
- Fora do escopo e NÃO tocado: `go.mod`/`go.sum` (bump de grpc indireto) e
  `.github/workflows/release.yml` apareceram modificados durante a sessão por trabalho paralelo
  do usuário. A divergência index/worktree de uma linha em `mandate.go` (a frase
  `re-open the skill for the domain you are re-entering`, removida no staged e presente no
  working tree) também é decisão do usuário — eu não editei essa frase em nenhuma direção.

## Segunda rodada — 2026-09-01, a pedido do usuário

Três itens que na primeira rodada ficaram como debt ou como decisão pendente foram
promovidos a trabalho:

- [ ] **T10 — `ORDER BY` e `LIMIT` sobre traversal, de verdade.** Spec: toca
  `internal/ast/ladybug_icebug_canonical.go`. A primeira rodada recusou por não existir
  estágio de ordenação; agora o estágio passa a existir. O tail do
  `canonicalTraversalPattern` consome os argumentos de `ORDER BY`/`LIMIT`, o plano carrega as
  chaves com direção, e `finishCanonicalTraversal` ordena o set materializado antes de aplicar
  o limite. Decisão de design: a chave de ordenação tem de estar PROJETADA (pelo texto
  `var.prop` ou pelo alias), porque ordenar por coluna não buscada exigiria alargar a projeção
  e depois escondê-la — recusa clara é melhor que projeção fantasma. Invariante: sem `ORDER BY`
  a ordem permanece a canônica reproduzível de hoje, e ela também é o desempate quando as
  chaves empatam.
- [ ] **T11 — Path absoluto e `File` duplicado no reindex incremental.** Spec: normalizar para
  relativo à raiz no caminho incremental, de modo que reindexar um arquivo editado não crie um
  segundo nó `File` nem entidades com `path` absoluto. Feito = editar um arquivo, deixar o
  watcher rodar, e `MATCH (f:File) WHERE f.path ENDS WITH '<arquivo>' RETURN f.path` responder
  uma linha relativa.
- [ ] **T12 — A frase do preamble sai de verdade.** O usuário confirmou que a remoção de
  `re-open the skill for the domain you are re-entering` é intencional. Então ela sai do working
  tree também, e `TestMandatePreambleReAppliesAfterAnInterruption` passa a exigir o que o
  preamble efetivamente diz sobre retomada, em vez da frase removida.
- [ ] **T13 — Devolver `ORDER BY` aos templates da skill** onde a ordem é semântica (colunas de
  índice composto, comentários por linha, skeleton de arquivo), e corrigir a linha da tabela do
  contrato que hoje diz que ordenação não existe.
- [ ] **T14 — Commit na main.** `go.mod`, `go.sum` e `.github/workflows/release.yml` são de
  outra sessão, confirmado pelo usuário: ficam fora do commit.

### 2026-09-01 — T10 a T13

**T10 — `ORDER BY`/`LIMIT` implementados.** O tail do `canonicalTraversalPattern` passou a
consumir os ARGUMENTOS, não só as palavras-chave — era isso que empurrava `ORDER BY x` para
dentro da projeção e produzia a recusa "not a plain property projection", que nomeava a regra
errada e escondia uma cláusula que o planner é capaz de honrar. O plano ganhou `orderBy []canonicalOrderKey`,
`limit` e `hasLimit`; `applyCanonicalOrdering` ordena o set materializado e trunca.

Decisões de design que valem registro:

- **A chave de ordenação tem de estar PROJETADA.** O sort roda sobre as linhas materializadas,
  não sobre a tabela, então ordenar por coluna não buscada exigiria alargar a projeção por trás
  do chamador e esconder a coluna extra depois. Recusa clara, com o fix na mensagem. Alias
  também serve como chave, e a propriedade subjacente resolve para a mesma coluna.
- **Números comparam numericamente.** `compareCanonicalValues` trata int/float antes de cair em
  string, senão `line_number` ordenaria 10 antes de 9 — que é o bug que uma feature de
  ordenação existe para evitar.
- **A chave canônica continua sendo o desempate final**, mesmo com `ORDER BY` presente: duas
  linhas iguais em todas as chaves voltariam na ordem que as queries batched produziram, que não
  é estável entre execuções.
- **`ORDER BY` sobre `count` é recusado** em vez de ignorado: contar devolve uma linha, e aceitar
  em silêncio uma cláusula sem efeito ensina que ela tem efeito.
- `TestOrderingDoesNotLoosenTheShapeRule` garante que ordenação é ADITIVA: label projetado,
  `DISTINCT` ausente, duas pontas e âncora sem filtro seguem recusados quando se acrescenta
  `ORDER BY`.

**T11 — causa raiz do path absoluto, e ela era uma assimetria.** `classifyBatch`
(`internal/daemon/syncmodule.go`) normalizava o path REMOVIDO para repo-relativo, com o
comentário dizendo que o parse cache é keyed assim, e passava o path ALTERADO cru, absoluto.
`internal/ast/watcher.go` tinha exatamente a mesma assimetria — dois callers independentes com
o mesmo defeito, o que decidiu onde o conserto pertence: no pipeline.

O mecanismo completo: `absToRel` era montado com `filepath.Abs(rel)`, que resolve contra o
**working directory do processo**. Na CLI o CWD é a raiz do projeto e tudo coincide. No daemon
não é — e o daemon serve vários projetos, então não pode fazer chdir para nenhum. Com
`ChangedPaths` absolutos, `filepath.Abs(abs) == abs`, então o mapa apontava absoluto para
absoluto, `correctRelPath` virava o próprio path absoluto, e
`r.pf.Path = filepath.Join(abs, correctRelPath)` produzia um path DUPLICADO. Daí as duas
observações da primeira rodada: `File.path` absoluto, e entidade com `home/lainosantos/...` sem
barra inicial, que é `writer.rel` do path duplicado.

Correção: `repoRelativePaths(root, paths)` normaliza qualquer das duas formas contra a RAIZ,
nunca via `filepath.Abs`. O que se LÊ passou a ser sempre `filepath.Join(abs, rel)` e só o que
se ARMAZENA é relativo. Efeito colateral bom: `fileHashes` agora é keyed absolutamente e o
lookup posterior acerta, onde antes errava e recomputava SHA-256 de todo arquivo do batch.
Path fora da raiz passa intacto — não tem forma relativa, e reescrever inventaria arquivo.

**Os outros dois incrementais foram auditados a pedido do usuário e estão IMUNES:**

| Módulo | Convenção do path incremental | Por que não sofre |
|---|---|---|
| AST | repo-relativo | era o afetado; corrigido |
| Knowledge | não recebe lista de path | `reindexKnowledge` reconstrói o wiki inteiro; `knowledgeSourceFile` deriva `relPath` de `filepath.Rel(absRoot, path)` com `absRoot` vindo do root passado, nunca do CWD |
| Memory | absoluto, **de propósito** | os paths do batch só alimentam `anyUnder`, um predicado que decide SE o escopo recompila; o compile é `RunCycle` sobre diretórios e nada do batch é persistido |

Os `os.Getwd()` em `internal/knowledge/paths.go` e `internal/memory/paths.go` são default de
`projectDir` quando o chamador não informa — o daemon sempre informa, então não entram nessa
classe. `internal/daemon/incremental_path_contract_test.go` fixa as TRÊS convenções, incluindo a
do memory, para que ninguém a "corrija" por analogia e quebre a seleção de escopo.

**T12 — a frase saiu.** O usuário confirmou que a remoção de
`re-open the skill for the domain you are re-entering` é intencional, então ela saiu do working
tree e `TestMandatePreambleReAppliesAfterAnInterruption` passou a exigir o que o preamble
efetivamente diz — `it re-applies all of it` e
`ahead of your native ones exactly as on the first turn`.

**T13 — `ORDER BY` de volta na skill** onde a ordem carrega significado: colunas de índice
composto em ordem de declaração (um índice composto serve query que lidera pela primeira
coluna), comentários e skeleton de arquivo em ordem de linha, parâmetros em ordem de assinatura,
imports em ordem de declaração. A linha da tabela do contrato que dizia que ordenação não existe
virou ✅ com a ressalva da chave projetada, e a forma única no texto ganhou a cláusula opcional.
Segue em 137 exemplos, 0 recusados.
