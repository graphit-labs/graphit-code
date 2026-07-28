# Cada skill passa a ensinar todas as ferramentas do próprio módulo

**Data:** 2026-07-28
**Escopo:** `internal/{ast,hub,knowledge,memory,improvements}/rule.go` e testes novos
**Origem:** etapa 1 de `docs/tasks/revisar-skills-e-mandates.md`

---

## `hub_search`: a ordem existia, o meio não

O mandate do Hub manda, sem exceção: *"antes de confiar no seu próprio conhecimento ou em busca
na web sobre QUALQUER framework/biblioteca/API externa, você DEVE primeiro checar o Hub via as
ferramentas MCP"*.

A skill do Hub **nunca mencionava `graphit_hub_search`**. O agente recebia a ordem de buscar e o
único caminho ensinado era `hub_list` → `hub_show` → `hub_install` — ou seja, listar o catálogo
inteiro e procurar com os olhos. Quem chega com um nome na mão ("a tarefa usa Stripe") tinha de
descobrir sozinho que existe uma ferramenta que aceita esse nome.

`hub_search` virou a **primeira chamada** em toda a skill: nas duas tabelas de precedência, nas
condições de fallback, na seção de uso e na regra final. `hub_list` continua, no papel que é
realmente dele — ler o catálogo quando você *não* tem um termo, ou quando a busca voltou vazia.

E documentamos como o casamento funciona, porque isso muda como se busca: o termo é comparado
como **substring** de id, nome e descrição, com id/nome ranqueando acima de descrição. Não é
semântico e não faz stemming. Daí duas regras novas com o porquê colado:

- Buscar o nome **que alguém registraria**, não só o nome do pacote.
- **Um resultado vazio não é resposta.** `fastapi` não encontra um artefato registrado como
  `python-web-frameworks`. Alargue o termo antes de concluir que o Hub não tem nada — virou
  anti-padrão explícito.

## Parâmetro errado, e uma afirmação que a ferramenta não sustenta

`hub_list` e `hub_search` **não têm `project_dir`** — o registry é global. Três skills mandavam
passar um:

- hub: `hub_list(project_dir: ...)` na seção "Installed Artifacts"
- knowledge: `hub_list(project_dir: "...", type: "knowledge")` no protocolo Hub-first
- ast: o mesmo, no passo 6 do fluxo de investigação

Pior que o parâmetro: a skill do Hub dizia *"para ver os artefatos instalados, chame `hub_list`"*.
`hub_list` devolve `reg.ListEntries()` — o que o **registry oferece**, não o que este projeto
instalou. Não existe ferramenta para isso; quem sabe é o `graphit.lock.json` na raiz. A seção
agora diz exatamente isso.

Um teste passou a proibir a reincidência: nenhuma skill pode conter
`graphit_hub_list(project_dir` nem `graphit_hub_search(project_dir`.

## O ecossistema de projetos ganha ensino de verdade

O Hub é metade do quadro: artefatos para instalar. A outra metade é o **ecossistema** — todos os
projetos registrados na máquina, agrupados por labels que o usuário controla. A skill tratava
`cluster_set`, `cluster_get` e `cluster_unset` como menção de passagem numa única frase, e o
`label` de `cluster_projects` não era documentado.

Agora a seção responde as duas perguntas que o código não responde:

1. **O que este projeto é, para o usuário?** As labels dizem em que domínio, time, stack ou tier
   ele foi arquivado. Isso é intenção, e não se infere da árvore de fontes.
2. **O que mais é relacionado?** Quais checkouts são irmãos, e onde estão no disco — é assim que
   "o serviço de auth" deixa de ser um nome e passa a ser um caminho consultável.

Com a semântica exata, que decide o que volta:

- Irmãos compartilham **ao menos uma chave *e* valor idênticos**. Mesma chave com valor diferente
  não casa.
- Uma label é chave → **vários valores**: um repositório pode ser `domain=billing` e
  `domain=invoicing` ao mesmo tempo. `cluster_unset` remove a chave inteira.
- **O projeto atual vem no resultado.** A skill anterior dizia "todos os projetos irmãos",
  induzindo o agente a ler a primeira entrada como sendo outro projeto.
- O mesmo projeto registrado em dois caminhos aparece duas vezes, a segunda com sufixo `#2`.
- Sem labels em ninguém, o grupo default é tudo.
- Um projeto só aparece depois de registrado — vazio pode significar "os irmãos nunca rodaram
  `init`", não "não existem".

E o que os caminhos permitem: `dir` é caminho absoluto real, então **todo** tool deste framework
aceita como `project_dir` — inclusive `memory_search` no projeto irmão, que costuma ser
exatamente o motivo pelo qual ele se comporta como se comporta.

Leia as labels à vontade; **mude só quando pedido** — relabelar rearranja silenciosamente o que
o ecossistema considera relacionado, e essa decisão é do usuário.

## As outras ferramentas ausentes

| módulo | o que entrou | por que importa |
|---|---|---|
| hub | `hub_uninstall`, `hub_submit`, `hub_projects`, `hub_type-path` | `hub_type-path` era usado pela skill de improvements e não era ensinado nem citado no mandate do próprio Hub |
| ast | `ast_list`, `ast_install`, `ast_remove`, `ast_index`, `ast_embed`, `ast_export` | a skill mandava *não* passar `context` no projeto próprio sem nunca dizer de onde vem um context |
| knowledge | `knowledge_list`, `knowledge_lint`, `knowledge_schema`, `knowledge_export`, `knowledge_sync` | `knowledge_sync` não estava nem no levantamento; é a ferramenta estreita para o caso em que o watcher não pode ter visto a mudança |
| memory | `memory_export`, `memory_schema`, `memory_remove`, `memory_sync` | `remove`/`sync` exigem `context` e agem em contexto importado — os nomes sugerem o contrário |
| improvements | `improvements_rules` | a metodologia da skill é um **default** que projeto ou artefato do Hub pode substituir |

Três decisões que o levantamento deixou abertas, e o motivo:

- **`ast_install`/`ast_remove` entram.** Não são só ciclo de vida: são a origem dos contexts que
  o resto da skill usa. Com aviso — `ast_remove` sem `context` executa
  `MATCH (n) DETACH DELETE n` no grafo do projeto atual.
- **`memory_sync` entra**, junto de `memory_remove`, numa subseção de contextos importados. A
  skill de memória não falava de contexto nenhum.
- **`knowledge_remove` entra por par**, com o mesmo aviso: sem `context`, limpa o wiki local.

`improvements_rules` ganhou seção própria com a razão: revisar contra o default quando existe
override é reportar como defeito justamente as escolhas que o projeto fez de propósito.

## Preferir a ferramenta estreita à ferramenta grande

Dois lugares mandavam chamar `graphit_sync` — que reindexa AST, wiki, memória e Hub — para
resolver um problema de um só subsistema:

- `ast_search` no modo semântico sem vetores → agora `ast_embed`
- wiki comprovadamente velho → agora `knowledge_sync`

Mesmo efeito, uma fração do trabalho.

## Testes

Um invariante por módulo, com o porquê no comentário: **toda ferramenta que o módulo possui tem
de ser alcançável a partir da própria skill**, senão o mandate anuncia capacidade que o agente
não consegue usar.

- `TestHubRuleContentTeachesEveryHubTool` (11 tools de hub + 4 de cluster)
- `TestHubRuleContentDoesNotInventAProjectDirOnRegistryTools`
- `TestMandateTriggerNamesTheSearchAndEcosystemTools`
- `TestASTRuleContentTeachesEveryASTTool`, `TestASTRuleContentExplainsImportedContexts`
- `TestKnowledgeRuleContentTeachesEveryTool`, `TestKnowledgeRuleContentPrefersTheNarrowSyncTool`
- `TestMemoryRuleContentTeachesEveryMemoryTool`, `TestMemoryRuleContentDistinguishesContextToolsFromDelete`
- `TestImprovementsRuleContentTeachesTheRulesTool`

`TestHubRuleContent` deixou de exigir um título literal — foi o que quebrou ao renomear a seção
do ecossistema. Agora verifica conteúdo, não redação.

`golangci-lint` limpo.
