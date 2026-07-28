# Watcher entra na CI das três plataformas, wiki 40/40, in-place cross-process e o fim da pista do Ladybug

**Data:** 2026-07-28
**Escopo:** `.github/workflows/ci.yml`, `internal/wiki/fts_embedding_test.go`,
`internal/ast/ladybug_crossprocess_test.go`, `internal/ast/ladybug_field_scale_test.go`,
`docs/upstream/`
**Origem:** pedido do Engenheiro para fazer o bloco "Trabalho de verdade"

---

## 1. macOS/Windows — o que dava, e o que não dava

**Não é possível executar daqui.** O que dá é fazer a CI executar para sempre, que é melhor
que uma rodada manual.

Descoberta ao olhar: `ci.yml` é **inteiramente `ubuntu-22.04`** — Lint, Tests, Security, Build
Check, UI. Os `macos-14` e `windows-2022` que aparecem no repositório são do `release.yml`, que
**constrói** release sem rodar teste. O watcher, cujo comportamento é fornecido pelo sistema
operacional (inotify, kqueue, ReadDirectoryChangesW) e que substituiu um poll de `git status`
que se comportava igual em todo lugar, nunca tinha sido executado fora do Linux.

Novo job `watcher-cross-platform`, matriz de três, `fail-fast: false`. Roda só
`internal/fswatch` e `internal/ignorer`, que dependem apenas de `fsnotify` e de dois pacotes
folha — sem CGO, sem ICU, sem LadybugDB — então não precisa de instalação de dependência por
plataforma e é rápido. `-count=1` para que verde signifique que rodou mesmo.

**Ressalva honesta:** o debounce dos testes é de 60ms e runner compartilhado é lento e
barulhento. A primeira execução pode ficar vermelha, e isso é o resultado, não um defeito do
job.

**ORT continua sem cobertura de execução.** É CGO puro; cross-compile daqui esbarra em
toolchain (`build constraints exclude all Go files`) e verificar de verdade exige a plataforma
com a biblioteca nativa instalada. O `release.yml` constrói nas três; executar, ninguém nunca.

## 2. FTS do wiki — 33/40 para **40/40**

As sete que faltavam eram o caminho de embeddings, e a razão de terem ficado de fora estava
errada: pareciam precisar de modelo. Não precisam — recebem e devolvem vetores, então vetores
sintéticos exercitam todos os ramos. O que se testa é armazenamento e ordenação, não qualidade
de embedding.

`unitVec(axis)` gera vetores unitários apontando cada um para um eixo, o que torna a geometria
trivial de raciocinar: uma consulta mirada num eixo tem que ranquear aquele chunk primeiro.

Cobertos: fila de pendentes drenando ao embutir, `EmbeddingStats` acompanhando, vizinho mais
próximo ranqueando certo, `topK` respeitado, chunk sem vetor **nunca** aparecendo em resultado
semântico, híbrido nos quatro estados (só texto, só vetor, ambos concordando, nenhum), e
rebuild não deixando vetor apontando para chunk que já não existe.

`optimizeTables` só roda a cada décimo rebuild — nove de dez execuções nunca chegam lá. O teste
faz dez e verifica que o índice continua utilizável depois da fusão de segmentos, que é o risco
real ali.

## 3. In-place com leitor cross-process

`TestLadybugSeparateHandleDuringWrite` já cobria segundo handle **no mesmo processo**. Produção
não é assim: o MCP é processo próprio, iniciado à parte, lendo enquanto o daemon escreve. A
afirmação "escrita in-place não quebra leitura sem trava" era in-process aplicada a um arranjo
cross-process.

Os leitores agora são subprocessos de verdade — o binário de teste se re-executa com
`GRAPHIT_XPROC_READER`.

```
writer: 33771 escritas, 0 erros
reader 0: 4932200 leituras, 0 anomalias
reader 1: 4917800 leituras, 0 anomalias
reader 2: 4944000 leituras, 0 anomalias
```

Cada linha é escrita como repetição limpa de um marcador, então leitura rasgada aparece como
corpo que não é repetição.

### O que a primeira tentativa revelou

Ela falhou com `failed to open database with status 1`, e a causa não era o teste estar
errado sobre o motor — era eu ter aberto os leitores read-write. **Um leitor que abre
read-write toma a vaga única de escritor e tranca o indexador fora.** Produção acerta
(`NewLadybugDBReadOnly`), mas nada garantia isso, e a falha aparece no indexador, longe do
processo que a causou, como um "failed to open database" opaco.

`TestLadybugReadWriteOpenerLocksOutTheIndexer` fixa os dois lados: com detentor read-only o
indexador anexa, com detentor read-write não.

## 4. Corrupção do Ladybug — a pista acabou

A escala de campo era a última dimensão não testada. Reproduzida exatamente: 35358 linhas,
866 MB de texto acentuado incluindo os caracteres de controle C1, lotes de 64,
`CREATE_FTS_INDEX` sobre a mesma coluna e varredura de todas as linhas.

```
índice construído limpo
35358 de 35358 linhas devolvidas
0 inválidas, 0 com tamanho errado
```

Com volume eliminado ao lado de forma do dado, concorrência e ponteiro cgo, **o 5º report
provavelmente está mal endereçado e não deve ser enviado como está.**

O que nenhuma sonda reproduz é o **caminho que as strings percorrem antes do banco**: em campo
elas saem de um parser, ficam num cache de shard, são serializadas para JSON e lidas de volta,
e chegam ao escritor com várias goroutines em voo. Toda sonda entrega ao banco uma string
montada ali mesmo, segundos antes.

Então o próximo lugar a olhar é **acima do banco** — o round trip do cache de parse e o
tratamento de string neste projeto — e não o liblbug. O report fica pelo valor das eliminações,
com essa conclusão escrita nele.

O teste é pulado por padrão (`GRAPHIT_FIELD_SCALE=1`), porque escreve ~1 GB e leva 2min13s.

## Estado

Suíte completa com `-race` limpa. `internal/wiki`: 40/40 funções de `fts.go`.
