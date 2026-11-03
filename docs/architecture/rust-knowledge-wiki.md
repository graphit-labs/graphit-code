---
type: architecture
title: Knowledge e Wiki
description: Contratos documentais ativos, produtores Rust e fronteiras de integração.
references:
  - target: {type: task, id: tsk-ab72}
    relation: implements
  - target: {type: task, id: tsk-45ca}
    relation: derived_from
  - target: {type: task, id: tsk-55e8}
    relation: describes
  - target: {type: task, id: tsk-08ae}
    relation: validated_by
  - target: {type: task, id: tsk-b210}
    relation: relates_to
  - target: {type: task, id: tsk-cdb6}
    relation: describes
  - target: {type: task, id: tsk-9c6e}
    relation: describes
  - target: {type: task, id: tsk-b96c}
    relation: validated_by
  - target: {type: task, id: tsk-729f}
    relation: describes
  - target: {type: task, id: tsk-c775}
    relation: validated_by
  - target: {type: task, id: tsk-84d5}
    relation: relates_to
  - target: {type: task, id: tsk-a0f9}
    relation: relates_to
  - target: {type: knowledge, id: Wiki_Module_Specification}
    relation: derived_from
  - target: {type: task, id: tsk-d9b2}
    relation: uses
  - target: {type: task, id: tsk-fa44}
    relation: uses
  - target: {type: task, id: tsk-9840}
    relation: uses
  - target: {type: task, id: tsk-5035}
    relation: uses
  - target: {type: task, id: tsk-cc0e}
    relation: uses
  - target: {type: task, id: tsk-1833}
    relation: uses
  - target: {type: knowledge, id: Filesystem_State_and_Watchers}
    relation: supports
  - target: {type: task, id: tsk-c9e2}
    relation: refined_by
---

# Knowledge e Wiki

Referência para implementadores de Knowledge/Wiki e seus consumidores. A entrega
`tsk-69f2`, na sessão `ses-c98a`, mantém dependências Data e AI pendentes. Este
documento distingue comportamento ativo inspecionado, produtor de biblioteca
Rust e integração ainda necessária. Não anuncia cutover CLI/MCP/HTTP ou conclusão
da migração.

Knowledge possui os contratos documentais, compilação, fontes, referências,
staleness, lint, busca e consumo/exportação de artefatos. Core, Auth, FS, Data,
Lance, Icebug, Ladybug, SCIP, Hub e AI/Runtime possuem seus respectivos produtores.
Um contrato ausente permanece pré-requisito; um store fictício não o substitui.

## Jornadas e contratos funcionais

| Jornada | Contrato funcional |
|---|---|
| Indexar documentos | Uma árvore configurada, padrão `docs/`, mais README opcional. Caminhos relativos ao projeto, ignores do projeto/árvore, deduplicação, README mesmo sem docs. |
| Compilar e organizar por domínio | Um corpo canônico por documento, metadados, tags, referências e comunidades. A compilação atual não exige inferência AI. Algoritmo de comunidades e domínio Rust ainda precisam de contrato. |
| Sincronizar sem perder integridade | Delta de inclusões/alterações/exclusões; embeddings inalterados preservados; histórico mantido. No-op não inventa alterações documentais; manutenção pode vencer mesmo sem mudanças. |
| Ler página e trecho | Corpo completo vem do índice, inclusive mount sem Markdown local. Cabeçalho renderizado e seleção textual comum; total de linhas e matches pertencem à página resultante. |
| Inspecionar referências e lint | Destinos ausentes permanecem disponíveis para diagnóstico; self-links e duplicatas não são arestas adicionais. Lint distingue conformance de recomendações. |
| Buscar e navegar | BM25, recuperação densa/híbrida, catálogo, predicados estruturados e múltiplas fontes. Cursores, limites globais e falhas de fontes devem ter contratos explícitos. |
| Consumir/publicar artefato atual | Mount somente leitura com grant vigente; abrir não cria/recompila/download de wiki. Exportação de pacote nativo e Markdown/OKF preserva proveniência. |
| Consultar Wiki AI | Catálogo e recuperação inicial sobre índice aberto; ciclo limitado solicita páginas e sintetiza resposta. É distinto da compilação e de LiveSearch. |
| Operar LiveSearch | Sessão no servidor com owner, estados, eventos e artefatos. Desconectar cliente não cancela o trabalho; cancelamento explícito, fechamento e reclaim são operações próprias. |

### Audit de compilação e conformidade do lint publicado

Audit de compilação e lint do índice publicado são operações distintas. A contagem de findings não comprova emissão de log, rejeição de publicação ou sucesso operacional de sincronização.

Campos authored ausentes não implicam campos compilados ausentes. A compilação pode derivar valores conforme sua policy aprovada, mas o leitor não deve inventar presença/autoria a partir de um valor efetivo. Ausência, null, vazio e whitespace precisam ser interpretados no contrato do campo.

O lint do índice consulta valores publicados, não o YAML original. Ele deve separar ausência de campo obrigatório de recomendação editorial e manter erro operacional distinto de findings válidos. O diagnóstico Rust abaixo define seus campos efetivos e contadores.

Os canais ativos também distinguem problema de qualidade de erro operacional:

CLI e MCP devem propagar erro operacional e apresentar os findings de um relatório válido sem confundir recomendações com autorização, sucesso de publicação ou erro de armazenamento.

A etapa Rust de [diagnóstico de metadados publicados](rust-knowledge-published-metadata-lint.md)
usa agora o documento canônico FULL16 de Data. Diagnostica `kind` efetivo vazio
como Required e `title`/`description` integral ausentes ou whitespace como
Recommended; somente Kind incrementa `required_errors`. O contrato nativo
separa description nullable de summary editorial: resumo derivado preenchido
não satisfaz description ausente, e resumo truncado vazio não invalida
description integral válida. Esses valores não reconstroem autoria de title/kind
nem distinguem absence/null de description. Audit de compilação e lint completo
(com corpo, referências, staleness e conformidade authored) continuam pendentes,
assim como o routing PROJECT CLI/MCP. A qualificação focal Linux passou:
seis testes do diagnóstico, seis da publicação real e um privado de overflow,
além de Clippy com warnings negados. Outros sistemas/routing App seguem UNRUN.

`tsk-0550` refinou a etapa publicada e `tsk-3a10` a implementa sobre os
produtores concluídos Data FULL16 e publicação nativa (`tsk-8b24`). O caller
retém a geração e o corpus após EOF autorizado com ReadKnowledge, e passa o
`SnapshotContext` original ao diagnóstico. O kernel limita páginas, bytes dos
campos examinados e findings, conserva causas e não retorna relatório parcial.
PublishedEmpty é válido em storage mas produz NoDocuments no diagnóstico;
NoCorpus permanece com Data/caller. Denial, cancelamento, limite ou leitura
incompleta nunca são lint limpo. Summary e description existem separadamente na API Rust; um campo não satisfaz automaticamente a regra do outro. O lint completo e seu canal App
seguem como outcomes distintos de `tsk-69f2`.

#### Proveniência e estados publicados

A pergunta mínima para o contrato existente de `tsk-b210`/`tsk-84d5` é: quais
distinções authored e origem por campo devem permanecer no documento canônico,
e quais estados efetivos cada campo admite na publicação/leitura? O compilador
precisa fornecer a proveniência aprovada; o leitor não pode inventá-la a partir
de strings defaulted. Digest dos bytes da fonte e identidade/fingerprint do
valor compilado têm propósitos distintos; igualdade de source hash não prova
igualdade de metadata/policy/modelo de compilação.

A proposta de interação é compiler → metadata efetiva/proveniência aprovada →
projeção Data da mesma origem/geração/schema, com completude, original
`SnapshotContext`, admissão real e custos agregados. Tipos, API e estados válidos
pertencem ao contrato compartilhado coordenado pelo principal. Esse refinamento
não entrega o modelo, grant, snapshot ou uma folha Source pronta.

Relações tipadas precisam de uma geração por fonte, incluindo marcador vazio após
remoção de todos os vínculos. Escritor antigo não pode restaurar relações removidas.
Reparo de projeção incompleta, compactação e retenção devem possuir limites e
evidência de retry/integridade. O adapter Data precisa provar corpus completo:
truncamento silencioso de uma consulta não constitui essa prova.

Importação de artefatos atuais continua no escopo. Não há importador de formatos ou armazenamento antigos. Exportação OKF conhece o ator em `generated.by`; uma
leitura que não possui essa informação não inventa proveniência.

## Configuração Knowledge entregue em Rust

`tsk-729f` entrega `KnowledgeSettings::resolve` sobre Core `ConfigSnapshot` e
`SnapshotContext` reais. A view owned expõe `docs_dir`, `include_readme`,
`extensions`, origens e contabilidade; defaults valem somente na ausência e um
winner presente inválido falha sem fallback. Docs default é `docs`, README é
incluído por padrão e as 16 extensões públicas estão no
[contrato de configuração](rust-knowledge-settings.md). Unicode é preservado;
CSV aplica ASCII casefold e dedup na primeira ordem. Os sete caps limitam
configuração/projeção, não enumeração, regras ignore, payload ou autoridade.

Os critérios resolvidos são enviados pelo consumidor ao Core. A configuração por si só não entrega o walker, a captura documental ou a ponte para Records/Data.

## Identidade nativa e seleção documental

A identidade e a captura físicas exigem paths Unicode válidos, sem conversão com
perda, normalização ou hash. Nomes selecionados não Unicode são erros explícitos.
Texto, título, slug e origem lógica continuam fronteiras distintas; um slug não
é chave física ou grant. As identidades canônicas `source-t-*` dos paths Unicode
permanecem iguais. A validação de conteúdo UTF-8 do parser também permanece.

Os produtores entregues oferecem peças com alcances diferentes:

| Produtor | Capacidade real | Fronteira para Knowledge |
|---|---|---|
| `tsk-d9b2`, `NativePath` | Path absoluto Unicode como string JSON limitada, com validação estrita. | Dado, não identidade física, captura ou tipo de path relativo selecionado. |
| `tsk-fa44`, `NativeDocumentDirectoryIdentity` | Identidade física completa do diretório, chave fixa de 33 bytes, comparação e revalidação da captura. | Record histórico não restaura FD/autoridade e não identifica um arquivo documental. |
| `tsk-9840`, `CapturedSourceReader::capture_project` | Captura física real de FILE ou diretório conhecido selecionado por path relativo, sob anchor PROJECT; conserva o `SnapshotContext` original recebido por `&Arc<SnapshotContext>`. | `SnapshotPolicy` seleciona retenção, não filtra payload antes da leitura/hash de subtree; `IgnoreScope` não está integrado ao walk. Não entrega seleção documental pré-payload CS8/C08. |
| `tsk-5035`, `NativeDocumentRouteProbeBatch` | Rotas explícitas finitas, ordem/duplicatas preservadas, contexto original e resultado integral Missing/Distinct/Same; corpo Distinct não lido. | Exige observação real presente do target vinculada à mesma request, parent físico e rawname; não descobre corpus ou target antes do payload. |
| `tsk-1833`, `PayloadSelection` | Path relativo nativo e kind como dados de seleção de payload existente. | Não enumera fontes, não captura identidade física nem define origem lógica documental. |
| `tsk-cc0e`, Hub `ProjectionPlanner` | Seleção finita de artefatos, fallback somente na ausência, enumeração limitada e ordem nativa antes do payload. | API sem SnapshotContext; não transportar policies de artefatos Hub ou usar como CoreSelection. |

A integração de arquivo PROJECT conhecido com o parser documental está ativada em
`crates/graphit-knowledge/tests/project_document.rs` e qualificada em Linux:
`cargo test --locked -p graphit-knowledge --test project_document` executou quatro
testes, todos aprovados, sem filtro;
`cargo clippy --locked -p graphit-knowledge --test project_document -- -D warnings`
também passou. A prova negativa correspondente passou no grupo de sete guards:
`graphit-knowledge/project_document` bloqueou o pre-commit com a falha sentinela;
a restauração passou o gate rápido. O recibo
`.build/rust-core-birth-anchor-20261009/root-qualification/seven-new-fast-guards-1791611469894397053/receipt.json`
(SHA-256 `d82ec2294d036a7410aff0f914fc7f5de3fd2aba18bbb6ad8f86cc489279e263`)
registra a execução concluída, sem alteração dos 308 arquivos protegidos. Windows
e macOS não foram executados nesta qualificação. Este resultado cobre
`capture_project` de FILE conhecido, request original, retenção e parser. O walker
documental seletivo antes de payload CS8/C08 e os controles capturados continuam
pendentes em `tsk-f814` e `tsk-b210`.

O route batch admite número de rotas, bytes nativos e access snapshots antes de
copiar nomes e abrir/sondar target e candidatos. A preparação ainda revalida o
parent e valida paths, incluindo metadata/identidade dos ancestrais; não promete
zero metadata antes dos caps. O máximo de snapshots é `2*n+2`, com overflow verificado.
Esse limite é por batch, não quota global de metadata, hard RSS ou limite de
syscalls. Probe revalida parent e target após cada candidato e ao final; ainda
não oferece snapshot atômico de todas as rotas, completude histórica ou prova
universal de ownership. Nome de rota de entrada não é nome observado de uma
entrada. `read_named` observa o nome depois de ler o payload; o batch não remove
essa fronteira nem permite fabricar observações por Path/serde.

A seleção documental continua no refinamento `tsk-b210`: enumeração de nomes e
metadata, ignores/control I/O ligados à request, decisão antes de abrir/hashiar
payload, conjunto selecionado opaco e revalidação completa. Capturar todo o
projeto e depois filtrar retenção não cumpre essa jornada. Ainda não há produtor
CoreDocumentSelection localizado que feche esse conjunto de garantias. Core
possui seu produtor; Knowledge consome o contrato quando entregue.

## Produtor Rust de referências resolvidas

`crates/graphit-knowledge/src/references.rs`, tarefa `tsk-55e8`, implementa uma
biblioteca sobre corpus completo emprestado. `PageReferences` contém slug, título
e destinos já resolvidos. Parsing Markdown, leitura de fontes, geração do corpus,
grants e persistência são responsabilidades dos produtores/integradores.

| API | Resultado e condição |
|---|---|
| `ReferenceGraph::build(context, pages, limits)` | Valida identificadores individuais; conserva destinos inexistentes e normaliza destinos vazios, self-links e duplicatas. Rejeita identidade de página duplicada. |
| `stats(context)` | Páginas, arestas únicas, páginas com inbound, órfãos e pares quebrados. Até estatísticas cached exigem requisição original viva. |
| `orphans(context, max_results)` | Slugs isolados, em ordem. Uma página com somente self-link é órfã; apontar para destino ausente conta como conexão outbound. |
| `broken_links(context, max_results)` | Todos os pares origem/destino ausente, ordenados por origem e destino. |
| `neighbors(context, slug, direction, depth, max_results)` | Uma direção fixa `Inbound` ou `Outbound`, profundidade 1–3, resultado ordenado por distância mínima e slug. Origem excluída, sem duplicatas/ciclos. |

Um destino ausente aparece numa consulta outbound com título `None`, mas não é
expandido nem aceito como origem de consulta. Página conhecida sem título usa seu
slug. Identificadores são UTF-8 opacos não vazios: sem `/`, `\`, caracteres de
controle ou prefixo `..`; não há normalização Unicode implícita.

As mudanças de API são deliberadas: direção fixa substitui caminhada mista; depth
inválido produz erro, não clamp; links quebrados preservam cada par, em vez de
selecionar uma origem arbitrária por destino. Por exemplo, `a → b ← c`, consultado
outbound de `a` com depth 2, retorna somente `b`. Se `a` e `c` apontarem para um
mesmo destino ausente, ambos os pares aparecem no diagnóstico. CLI/MCP futuros
devem adaptar apresentação de direções e tipo da referência explicitamente.

### Limites e erros

`ReferenceLimits` contém somente `max_identifier_bytes: NonZeroUsize`.
Slugs, identificadores consultados e destinos não vazios validam esse limite
individual de bytes UTF-8. Não há limite de páginas, edges raw ou soma textual
no grafo. Normalização, duplicatas, self-links, dangling e stats funcionais
continuam preservados.

Consultas que retornam listas recebem máximo positivo de resultados. Exceder
qualquer limite gera `LimitExceeded` com a fronteira identificada; não há truncamento ou relatório
parcial. `InvalidIdentifier`, `DuplicatePage`, `UnknownPage` e `InvalidDepth`
distinguem entrada inválida de ausência. Erros de alocação do vetor de resultados
são `Request` com `OutOfMemory`; as coleções padrão não oferecem garantia de RSS
ou recuperação universal de falha do alocador.

O grafo retém `SnapshotContext` original do Core. Construção e consultas verificam
cancelamento/prazo cooperativamente, inclusive antes de devolver resultado. Outra
referência de contexto precisa satisfazer `same_request`; igualdade de deadline,
signal ou dados não estabelece identidade. `Arc::clone` do contexto original é aceito.
Cancelamento preserva `Interrupted`, prazo preserva `TimedOut`; contexto de outra
requisição produz `DifferentRequest`. A biblioteca não faz I/O nem reserva seu
orçamento. Identidade da request não concede acesso nem prova geração do corpus.

As contagens de stats descrevem o grafo completo; não são saldos de quota.
Identificadores e resultados de queries mantêm seus limites reais. Comparações
nas coleções e ordenação são síncronas; os checks cooperativos não preemptam uma
operação individual. Texto permanece emprestado. O caller deve controlar memória
do corpus, autorização, geração e vida útil fora desta biblioteca.

### Verificação da folha

Os resultados e receipts anteriores são históricos e permanecem intactos.
A mudança de limites em `tsk-c9e2` foi qualificada em Linux: 157 funções
da biblioteca e dos 12 targets afetados, Clippy estrito nos mesmos targets,
compilação das duas fixtures App e três doctests modificados passaram.
O recibo `.build/rust-knowledge-reference-limits-cleanup/validation-current.json`
separa as falhas iniciais de compilação, o replay de referências e a evidência
atual. Windows e macOS não foram executados.

As fixtures em `tests/references.rs` cobrem corpus com dangling/self/duplicatas,
ciclos, direção fixa, todas as origens quebradas, permutação de entrada, UTF-8,
limites exatos/excedidos, queries inválidas, cancelamento e prazo, request distinta
e `Arc::clone` do contexto original.

```text
cargo test --locked -p graphit-knowledge --lib --test references
cargo test --locked -p graphit-knowledge --doc
cargo clippy --locked -p graphit-knowledge --all-targets -- -D warnings
```

## Fronteiras ainda necessárias

| Task | Resultado do refinamento | Produtores e dúvidas |
|---|---|---|
| `tsk-b210` | Modelo, parsing por biblioteca madura, slugs/colisões, compilação/domínios, fontes/trechos, renderização e staleness | Core captura/readers/text_slice existentes; descoberta com ignore, caps de parser e compilação ainda exigem produtores qualificados. Kernel puro de staleness validado em `tsk-9c6e`; integração persistida continua pendente em `tsk-84d5`. |
| `tsk-84d5` | Snapshot completo, delta, busca/cursores, relações, export/import e lifecycle | Data Snapshot/FTS/dense/writer/publicação/fencing; Core/Hub/Auth para acesso. Icebug sample/vizinhança não substituem essas APIs. |
| `tsk-a0f9` | Wiki AI e LiveSearch reais | AI/Runtime para cliente, turnos/contexto/eventos/sessões; Hub/Core para artefatos, ownership, cancelamento e cleanup. |

### Staleness: biblioteca pura validada e contrato

A comparação de staleness usa o snapshot publicado e observações atuais completas. Propagação inbound transitiva, relógio explícito, remoção tipada de fonte e limpeza de drift após recompilação consistente são fronteiras distintas; recompilar uma página não deve imediatamente marcá-la stale com o estado anterior.

O módulo `crates/graphit-knowledge/src/staleness.rs` implementa a API pura
`assess(context, graph, pages, observations, evaluated_at, limits)`
com `ReferenceGraph` e `SnapshotContext` reais. Fonte removida é uma observação
`Removed`; falta de observação é erro de integridade, não remoção presumida.
Em `a → b → c`, uma divergência entre o hash publicado e atual de `c` torna as
três páginas stale. Publicar um corpus consistente elimina essa divergência.
Idade é uma regra explícita separada, com limiar e timestamp fornecidos.

Resultados `StalePage { slug, since, reason }` possuem uma causa principal determinística: causa direta prevalece;
dependências usam a menor raiz por slug. Atravessar todas as bordas conhecidas
não herda o limite de profundidade 1–3 das consultas de apresentação. Ciclos
terminam e destinos dangling não viram páginas. Contagens, bytes, passos de
travessia e saída têm limites positivos e falham integralmente; não há I/O ou
relógio interno. Igualdade de slugs e hashes não prova grant, geração ou captura.
O adapter Data continua responsável por essas garantias. As correções de
`tsk-b210` foram reconciliadas no contrato `tsk-9c6e` r3 antes do código; a folha
consome `tsk-55e8` concluída. A validação da biblioteca pura não substitui integração Data, seus consumidores ou execução em outra plataforma.

`PublishedPage` fornece slug, fonte opcional `SourceFingerprint { path, digest:
[u8; 32] }`, `updated_at`, `stale_after` e `stale_since` opcionais. Cada fonte
publicada exige uma `SourceObservation { path, state: Present(digest) | Removed }`.
Fontes novas ainda não publicadas podem constar das observações: são validadas e
contam os limites, mas não geram páginas stale. Paths relativos UTF-8 usam `/`;
absoluto, drive/colon, backslash, controle, componentes vazios, `.` e `..` são
rejeitados, sem normalização de host ou leitura do FS.

Fonte alterada/removida prevalece sobre Age. Mesmo quando a fonte vence, toda
metadata temporal é validada: `updated_at`/`stale_since` posteriores ao relógio
fornecido e overflow da expiração são `InvalidTimestamp`; idade zero ou idade sem
update são `InvalidAgeLimit`. Tempos anteriores ao Unix epoch são válidos. Age
vence quando `updated_at.checked_add(stale_after) <= evaluated_at`. Sem since
anterior válido, Age usa a expiração; fonte e dependência usam `evaluated_at`,
inclusive dependência de uma raiz Age. Um since anterior válido é preservado
enquanto houver causa; sozinho ele não impede a limpeza do drift.

Por exemplo, `updated_at=90s` e idade `10s` ficam fresh em `99s`, stale em `100s`
e mantêm since `100s` em `120s`. Um dependente sem since anterior usa `120s` nessa
última avaliação. Uma fonte removida usa `SourceRemoved { path }`, enquanto os
dependentes usam `Dependency { root_slug, root_cause: SourceRemoved { path } }`.
Digest diferente gera a causa equivalente `SourceChanged`. O doctest público
mostra remoção com contexto Core real; `tests/staleness.rs` cobre mudança e Age.

`StalenessLimits` exige limites `NonZeroUsize` explícitos: páginas, links do grafo,
observações, bytes por path, bytes agregados de metadata e resultados.
Metadata conta os bytes UTF-8 brutos de cada slug, path publicado e de **todas**
observações antes dos mapas, incluindo duplicatas e extras. O limite de links
inclui dangling. Contagens exatas são aceitas; exceder qualquer limite retorna
`LimitExceeded(boundary)` e descarta a resposta inteira. Os rótulos são `pages`,
`graph edges`, `observations`, `path bytes`, `metadata bytes` e
`results`.

`graph.stats(context)` valida primeiro a request original. O corpus deve conter
exatamente os slugs únicos do grafo: duplicatas são `DuplicatePage`, cardinalidade
ou origem ausente são `CorpusMismatch`. Cada página consulta a API pública
`neighbors(Inbound, depth=1)` uma vez, retendo um cache limitado. Raízes são
percorridas em ordem UTF-8 com visited global: cada página entra na fila no máximo
uma vez e cada aresta inbound é examinada no máximo uma vez. Esse controle evita
trabalho repetido e encerra ciclos. Atravessar outra raiz preserva sua causa direta,
mas permite propagar a menor raiz aos demais dependentes. Os checks do contexto
original preservam cancelamento e deadline durante a propagação.

Observações duplicadas são `DuplicateObservation`, fonte sem observação é
`UnobservedSource` e path inválido é `InvalidPath`. Erros do grafo e de request
mantêm a origem em `References(ReferenceError)` e `Request(io::Error)`. Checks
cooperativos por unidade preservam cancelamento/deadline; outro contexto não
substitui a request. A avaliação usa o contexto original e limites próprios
para páginas, fontes, ligações e resultados emprestados.
Reservas de Vec são fallíveis, mapeando falta de memória para `OutOfMemory`;
coleções std e limites lógicos não prometem recuperação formal de todo OOM, RSS
rígido ou preempção. Esta API não escreve stale, não reserva I/O e não renova
deadline.

### Proposta de parser e contratos necessários

O refinamento `tsk-b210` originou a folha independente `tsk-cdb6`, com
[contrato e fonte candidata do parser](rust-knowledge-document-parser.md).
[pulldown-cmark 0.13.4](https://docs.rs/pulldown-cmark/0.13.4/pulldown_cmark/struct.Parser.html)
fornece eventos Markdown com offsets; Granit 1.3.0, consumido pelo reexport de
serde-saphyr 1.3.0 já locked, fornece eventos YAML, ranges de bytes e diagnósticos.
Coleções compõem abertura até fechamento; scalars conservam o span nativo,
inclusive valores omitidos sintetizados com range vazio ou do delimitador.
A fonte integral e os offsets permanecem originais. O parser usa arena plana,
sem resolver de schema YAML nem tipo de referências Records concorrente.
Módulo e dependências foram ativados conjuntamente pelo principal. Run4 Linux
aprovou 16 unitários, três integrações públicas e Clippy estrito, sem testes
falhos ou ignorados. Quinze hashes antes/depois foram preservados, todos os
processos terminaram e a CPU foi devolvida ao principal. O guard Git Integration
`graphit-knowledge/document` foi qualificado: três testes públicos PASS e
exatamente uma falha sentinela `broken_fast_contract`, Cargo EXIT101 e
`git hook run pre-commit` EXIT1, com restauração positiva das 37 fases e 929
arquivos copiados invariantes. O recibo
`.build/rust-core-birth-anchor-20261009/root-qualification/two-new-current-guards-1791626157505598000/receipt.json`
(SHA-256 `1b1a0bf7ecfb90df338c192d97263b4f6ea4c6d177a5e613bcdcf4185cee3074`)
preserva a prova específica, isolamento, filhos diretos aguardados e scratch
vazio. A Source Run4 permanece intacta; o grupo inicial FAULT segue histórico.
Essa qualificação pura não substitui compilação documental, Records/Data,
descoberta ou integração de corpus.

| Fronteira proposta | Regra e prova necessária |
|---|---|
| Entrada e metadados | Caps positivos de bytes de documento/frontmatter/input agregado com fallback, eventos YAML/Markdown separados, profundidade YAML, escalar, links brutos e ledger cumulativo de textos retidos (inclusive borrowed) e cópias da projeção. Resumo tem cap editorial UTF-8 separado. Contadores checked e fixtures exato/+1. |
| YAML | Abertura inicial exata `---`, após uma BOM opcional, LF/CRLF/EOF (opener EOF aberto retorna erro); fechamento exato `---` ou `...`. Vazio é metadata vazia; bloco aberto sem fechamento é erro. Um mapping; YAML inválido falha integralmente, inclusive sem `references`. |
| Aliases e estruturas | Proposta recusa anchors, aliases, merge `<<` e tags explícitas. Eventos sem `YamlLoader`; keys escalares e duplicatas decodificadas rejeitadas em todo mapping. Não confundir símbolos em prosa com eventos YAML. |
| Corpo e links | Preservar corpo fonte e ranges Markdown originais. Parser só extrai sintaxe e partes protegidas; autolinks, aliases e self-link exigem folha posterior com slug/corpus/resolver. Opções propostas: tables e wikilinks explícitos. |
| Requisição e integridade | Contexto recebido real, cancelamento/prazo antes/depois e entre eventos; erro descarta projeção. Bytes não provam autoridade/identidade da captura; isso cabe ao chamador. Bounds lógicos não prometem hard RSS ou preempção da chamada de parser. |

Referências tipadas possuem três estados: ausência preserva vínculos authored,
lista preenchida substitui e `[]` limpa. Essa semântica é definida em `Persisted_record_relationships`. O tipo canônico
Rust compartilhado de Records/Data permanece pendente; Knowledge não cria uma
representação de armazenamento concorrente para tornar o parser aparentemente
pronto.

Core já oferece `IgnoreScope::load/child/is_ignored/should_descend` e captura/readers
reais. A descoberta documental ainda precisa ligar essas capacidades ao conjunto
capturado, aos ignores observados e à request original, incluindo mudança entre
enumeração/captura, symlink/escape, ausência legítima de docs e README, caps e erros.
Um fingerprint serializado não substitui essa autoridade. Domínio, estabilidade
de identidade/colisões, biblioteca de comunidades e schema de renderização ainda
precisam de packets finais antes da compilação documental completa.

A especificação Wiki mantém seções antigas sobre BM25 próprio, arquivos e ciclo
AI em disco junto dos contratos mais recentes de Lance/corpo único. A consulta AI
atual lê o índice; a antiga whitelist de compilação de várias árvores LiveSearch
foi retirada. Essas narrativas não justificam reconstruir armazenamento/helpers
antigos. A substituição de cada comportamento ativo precisa de prova do consumidor
e documentação na própria unidade.
