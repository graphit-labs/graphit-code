# `config`, `daemon` e `dream` ganham cobertura — sem sexto mandate

**Data:** 2026-07-28
**Escopo:** `internal/hub/rule.go`, `internal/knowledge/rule.go`, `internal/ast/rule.go`,
`internal/improvements/{rule,rules}.go` e testes novos
**Origem:** etapa 2 de `docs/tasks/revisar-skills-e-mandates.md`

---

## A decisão de arquitetura

Três opções estavam na mesa: skill própria para cada domínio, uma skill "operações" cobrindo os
três, ou seções dentro das existentes. **Escolhi a terceira**, e não por economia de esforço.

### O caro é o mandate, não a skill

O bloco de mandate fica em contexto em **toda** sessão. O corpo da skill é carregado sob demanda.
Um sexto bloco permanente para 11 ferramentas de uso ocasional é troca ruim — e o mandate é
justamente o recurso escasso deste desenho.

### Cada domínio já tinha uma skill que levava o agente até a porta e o abandonava

Isto foi o que decidiu. Não é "onde essa ferramenta se encaixa", é **o que o agente está fazendo
quando precisa dela** — e as três respostas apontavam para skill existente:

| domínio | skill | o gatilho que já existia sem mecanismo |
|---|---|---|
| `dream` | improvements | *"você notou algo que vale corrigir fora da mudança atual"* — e a skill não oferecia saída nenhuma |
| `daemon` | knowledge | a tabela de exceções abre com *"o daemon não está rodando"* — e não havia como checar |
| `config` | hub | o hub já é dono de `cluster_*`; configuração é a mesma vaga: o framework, seu registry, seus projetos |

### Precedente

As skills daqui já são agrupadas por **gatilho**, não por prefixo de ferramenta: hub é dono de
`cluster_*`, knowledge é dono de `wiki_*`. Uma skill por prefixo de ferramenta seria a novidade,
não o contrário.

---

## `dream` → improvements: a terceira saída

Todo review encontra mais do que a mudança atual deveria carregar. Antes disso o achado tinha
duas saídas, ambas ruins: **alargar o diff até ficar irrevisável**, ou **mencionar em prosa e
perder**. A frontmatter da skill já prometia "dream subjects" e "schedule work for later
autonomous processing" — o conteúdo nunca entregou.

A seção nova ensina as cinco ferramentas com as duas precondições que decidem se um subject
serve para algo:

**O agente do dream não herda esta sessão.** Não sabe em que arquivos você estava, o que o
usuário disse, nem por que importa. Um subject que diz *"corrigir a duplicação que discutimos"*
é um subject que não produz nada. O `body` é o briefing completo — caminhos, sintoma, o que já
foi descartado, como saber que funcionou. Mesmo padrão dos task logs.

**Dream é opt-in.** `modules.dream` ausente significa desligado. Então **não prometa ao usuário
que algo será feito à noite sem olhar `dream_status`**: com `enabled: false` ou
`daemon_running: false` o subject fica lá para sempre e o achado se perde exatamente como se você
não tivesse dito nada. O Reflection Summary ganhou seção `### Deferred` que exige responder
*"alguma coisa vai pegar isso?"*.

E uma armadilha de leitura: **`dream_reports` marca como lido**. A chamada default devolve os
novos *e avança o marcador*, então a segunda chamada volta vazia — os relatórios não
desapareceram. `all: true` para revê-los.

## `daemon` → knowledge: a pergunta que a tabela fazia sem responder

A primeira linha da tabela de exceções — *"o daemon não está rodando → nada está observando"* —
é uma **pergunta**, e a skill não dava como respondê-la. `daemon_status` responde, sem
`project_dir` porque o daemon é global.

Com leitura campo por campo: `running: false` explica **todos** os sintomas de índice velho da
seção de uma vez; `uptime_seconds` posterior à sua edição significa que o watcher nunca a viu;
`recent_logs` é onde um rebuild que falhou diz por quê.

**"O wiki está velho" e "o daemon caiu" são indistinguíveis de onde o agente está, e só um dos
dois é bug.** Checar antes de reportar.

### A falha transitória que se disfarça de índice ausente

Descoberta empiricamente nesta sessão: com o daemon rodando, uma leitura do grafo que cai na
janela em que ele mantém o write lock falha com

```
ladybug open: failed to open database with status 1
```

A mensagem **nomeia o banco**, então lê-se como "não existe grafo aqui". É lock, não ausência —
a mesma consulta funciona segundos depois. Um agente que acredita na mensagem cai para grep,
abandonando o grafo justamente porque ele estava ocupado se construindo. Documentado nas duas
skills, com o contraste que desambigua: índice de verdade ausente reporta
*"no AST database found at ..."*, texto diferente.

`daemon_stop` entrou com o porquê colado: para o reindex automático de **todos** os projetos da
máquina e leva as sessões de dream com ele — depois disso tudo que a skill afirma sobre
"reindexação é automática" deixa de ser verdade. Só quando o usuário pedir.

## `config` → hub: quase todo "por que o framework fez isso" tem resposta aqui

A seção é diagnóstica, não tutorial. Situação observada → chave que explica:

| o que se observa | chave |
|---|---|
| o wiki indexa arquivos inesperados | `knowledge.docs_dir` — **default é `.`, o projeto inteiro**, não `docs/` |
| as ferramentas de um módulo devolvem nada e nada parece quebrado | `modules.<nome>` |
| `ast_source` não tem fonte de um arquivo indexado | `ast.index_source` |
| nada acontece de madrugada | `modules.dream`, opt-in |

E as três armadilhas:

**Precedência.** Inline → **variável de ambiente** → projeto (`graphit.lock.json`) → global
(`~/.graphit/config.json`) → defaults compilados. Consequência prática: um valor pode estar em
vigor com `config_list` mostrando nada, porque env var ganha dos dois arquivos e não aparece em
nenhum. Config listada contradizendo comportamento observado = **suspeite de env var antes de
bug**.

**`config_get` responde em prosa quando a chave não existe** — devolve a frase `Key "x" is not
set locally.`, não erro nem valor vazio. Não repasse isso como se fosse a configuração.

**`modules.<nome>` lê ao contrário.** `"false"` desabilita, `"true"` habilita. E ausente não é
igual a `"true"`: para módulo opt-in, ausente é desligado.

Escrita com escopo explícito: `global: true` muda **todos** os projetos da máquina — nunca por
iniciativa própria.

## Mandates

Cada módulo recebeu as ferramentas novas no inventário e gatilhos concretos:

- **hub** — *"um módulo deste framework se comportou de um jeito que você não sabe explicar — leia
  a configuração antes de chamar de bug"*
- **knowledge** — *"um índice parece velho, ou uma leitura do grafo falhou ao abrir o banco —
  descubra se o daemon está vivo antes de concluir qualquer coisa"*
- **ast** — *"uma leitura do grafo falhou ao abrir o banco: isso é lock, não índice ausente;
  tente de novo antes de cair para outra ferramenta"*
- **improvements** — o gatilho de achado fora de escopo agora diz *"existe ferramenta para isso,
  não precisa ser descartado nem enfiado à força"*

## Testes

- `TestImprovementsRuleContentTeachesDreamSubjects` / `...WarnsAboutDreamPreconditions`
- `TestKnowledgeRuleContentTeachesDaemonStatus` / `...ExplainsTheTransientLock`
- `TestHubRuleContentTeachesConfiguration` / `...ExplainsTheConfigTraps`

Os que verificam aviso, e não só menção, existem porque a menção sozinha não evita o erro: citar
`dream_subject_add` sem dizer que o agente do dream não herda a conversa produz subjects inúteis.

`golangci-lint` limpo.
