---

title: Leituras Knowledge PROJECT no browser Rust
doc_type: spec
description: Contrato de heads e página nativa por seleção exata, com autorização Knowledge própria e saída limitada.
references:
  - target: {type: task, id: tsk-4e50}
    relation: implements
  - target: {type: task, id: tsk-c135}
    relation: implements
  - target: {type: task, id: tsk-72ce}
    relation: uses_api
  - target: {type: task, id: tsk-5a58}
    relation: uses_api
  - target: {type: task, id: tsk-8e5d}
    relation: uses_api
  - target: {type: task, id: tsk-6062}
    relation: uses_api
  - target: {type: task, id: tsk-ef75}
    relation: uses_api
  - target: {type: task, id: tsk-db07}
    relation: implements
  - target: {type: task, id: tsk-ef75}
    relation: relates_to
  - target: {type: task, id: tsk-69f2}
    relation: implements
  - target: {type: task, id: tsk-9d92}
    relation: implements
  - target: {type: task, id: tsk-51f2}
    relation: implements
  - target: {type: task, id: tsk-67e7}
    relation: uses_api
  - target: {type: task, id: tsk-c135}
    relation: uses_api
---

# Leituras Knowledge PROJECT no browser Rust

Current path contract: PROJECT roots and relative paths cross JSON/API/CLI as literal Unicode strings. Serde preserves spaces, accents, escapes and separators without lossy conversion. Non-Unicode input fails explicitly; `{os,units}` is rejected. Historical evidence below describes the code that was exercised then and does not qualify the removed path codec.

## Publicação progressiva de fontes e embeddings

O watcher publica cada arquivo documental completo em `index.lance` antes de
processar o próximo. Source, heads, referências e FTS passam a consumir essa
geração imediatamente pelos ingressos CLI/MCP/HTTP/Browser existentes. O fim do
corpus e a disponibilidade do serviço de modelos não condicionam essas leituras.
Uma falha posterior conserva a publicação confirmada e sua causa; uma seleção
com erro não autoriza remover arquivos que ainda não foram examinados.

Eventos incrementais preparam somente os caminhos alterados. O rescan verifica
a seleção e compara SHA físico e receita de compilação já persistidos, sem
repetir o parser de arquivos intactos. Remoções são aplicadas às origens realmente
ausentes após concluir a seleção. Frontmatter, referências explícitas, datas,
controles de leitura, autorização e a integridade de cada arquivo continuam sob
os mesmos produtores.

A inferência recebe apenas slugs já confirmados e lê a seleção autorizada do
armazenamento. Source usa o cancelamento de vida do job; embeddings usam o
cancelamento de background, permitindo admitir novas alterações sem perder
fontes publicadas. Cada arquivo representa atualmente um documento e um vetor:
o lote real desse documento é confirmado antes do próximo arquivo. O produto
não divide documentos em chunks novos para simular checkpoints. Textos acima
dos limites existentes de arquivo ou modelo continuam produzindo erro explícito.
Excluir um documento remove seus vetores após confirmar a remoção Source, sem
carregar modelo nem executar RPC. Tombstones existentes permitem retomar essa
limpeza se ela falhar depois do commit documental.

A espera por slugs não mantém captura física nem admissões adicionais de
embeddings. Ao receber cada arquivo, o consumidor recaptura o projeto e a versão,
autentica o caller e admite leitura e escrita antes de acessar metadados ou
inferência. Essas capturas são liberadas antes da próxima espera, inclusive
quando o arquivo já tem vetores compatíveis. A identidade do modelo é consultada
uma vez por stream; o contexto Original e o cancelamento permanecem os mesmos
durante o job. A captura do produtor Source continua viva enquanto necessária à
publicação documental. Isso remove retenção ociosa de descritores sem criar
quotas, alterar NOFILE ou dispensar autorização entre arquivos.

Vetores são reutilizados somente com o mesmo conteúdo projetado, identidade de
modelo, revisão, receita e dimensões. Retomada de NN e mudança de modelo usam
Source já salvo, sem reparse. Uma troca incompatível reinicia normalmente o
espaço vetorial no primeiro lote real novo; resultados semânticos podem ter
cobertura parcial enquanto os demais documentos são calculados. A consulta
admite apenas os documentos cujo fingerprint e identidade ainda correspondem a
Source. Nenhum resultado parcial comprova cobertura do corpus inteiro.

O browser lê o Knowledge nativo do projeto e da versão selecionados. O servidor
admite `Knowledge/read` pelo Broker antes de abrir o corpus. Um JWT válido ou uma
autorização Task/Memory não concedem esse acesso. Essas rotas são somente leitura.

## Requisição e seleção

`POST /api/knowledge/heads` recebe:

```json
{
  "selection": {
    "project_dir": "/projetos espaço/café Ω",
    "project_id": "ID nativo do projeto",
    "project_version": "main"
  },
  "input": {}
}
```

`project_dir` é o `NativePath` literal retornado pelo inventário do servidor; o
exemplo acima é ilustrativo. ID, caminho e versão precisam identificar juntos um
contexto já retido. Não há resolução por nome, branch inferida ou busca de checkout.
Versões são strings literais não vazias, sem NUL.

`POST /api/knowledge/page` usa a mesma seleção e `input: {"slug": "nome literal"}`.
O validador nativo rejeita slug vazio, separadores, controles e prefixo `..` antes
de I/O. Campos desconhecidos ou duplicados são inválidos. Slug não é um caminho
para um arquivo nem uma concessão de acesso a outro contexto.

As rotas usam a sessão opaca, Origin exato e validação local do access JWT das
[sessões browser](rust_browser_auth.md). UserInfo indisponível não invalida um
access token válido; PROJECT é uma decisão separada e fresca por requisição.

## Resposta nativa

As respostas de sucesso de heads e page têm `{selection, source, focused_source, native}`.
`source` é `{kind:"project_knowledge"}`. A seleção é a tupla
efetivamente admitida. `native.state` distingue `no_corpus` de `published`.
Somente `published` inclui `generation` com `corpus_uri`, `version`,
`physical_branch` e `inherited_target`. URI e pin são observações, sem autoridade.

Heads retorna `heads`, ordenados pelo slug literal, com `slug`, `title` e
`updated_at`. A consulta usa a projeção nativa de metadata, sem decodificar
bodies nem a semântica dos campos não projetados.

Page retorna `page.state`:

| Estado | Significado |
| --- | --- |
| `published_empty` | Há uma geração publicada sem documentos. |
| `page_absent` | A geração não possui a página solicitada. |
| `found` | `document` contém a página nativa completa. |

A página preserva os campos canônicos do DTO Data: slug, título, body, kind,
summary, description, summary_truncated, source_path, physical_digest,
compilation_digest, recipe, recipe_parameters, updated_at, stale_since,
stale_after e resolved_wiki_targets. Strings vazias e `null` continuam distintos;
targets mantêm a ordem e repetições nativas. O viewer não recompila Wiki.

Digests são arrays de 32 bytes. Datas são strings decimais de nanossegundos
assinados desde Unix epoch, preservando nanossegundos, datas anteriores ao epoch
e `null`. `stale_after` é `{seconds, nanoseconds}` ou `null`.

## Integridade, limites e entrega

Page usa uma única pin e percorre a geração até EOF, validando todos os documentos
antes de devolver `found`, ausência ou vazio. Só o body selecionado permanece
retido; heads retém apenas metadata. Uma cauda inválida não vira prefixo saudável.

O input fechado usa uma única decodificação Serde, sem DOM intermediário ou
contabilidade de nós/profundidade. `max_input_bytes` limita o corpo antes do
decode; versão e slug são medidos em bytes UTF8 decodificados por
`max_scalar_bytes`. IDs são ULIDs nativos canônicos; os caminhos são strings
Unicode literais, limitadas por `max_scalar_bytes` e controles de formato/NUL/tamanho.
`max_collection_entries` limita somente coleções reais.
Os limites positivos do leitor continuam governando páginas, edges, texto por
documento, arquivos nativos e batch rows. Heads valida o tamanho de cada slug.
Não há reserva, quota,
contabilidade adicional ou retenção de corpus em JSON. Views borrowed serializam
uma vez em um Vec limitado pelos bytes reais de saída, que é movido ao body HTTP.
O codec privado também atende Memory; não cria API pública adicional.

A admissão e o leitor usam a mesma Original finita e cancelável. Autoridade,
captura física e Original são rechecadas antes da entrega; parada e trabalhos
aceitos continuam sob os owners existentes até join. Disconnect pode deixar o
handler já aceito retido pelo transporte até seu deadline finito.

| Condição | Resultado |
| --- | --- |
| Cookie ou JWT inválido/expirado | HTTP401. |
| Origin errado ou PROJECT negado/provider removido | HTTP403. |
| Body/slug/campo inválido | HTTP400; corpo acima do limite HTTP413. |
| Seleção bem formada, mas não retida | HTTP404 antes de PROJECT. |
| Broker responde grant de outro módulo | HTTP502, sem acesso ao SDK. |
| Corpus inválido, SDK ou limite de leitura/saída | HTTP500 causal, sem `native` saudável. |
| Página e referências do mesmo corpus Knowledge observadas em gerações diferentes | HTTP409 `knowledge_generation_changed`, sem resposta parcial ou retry automático. |
| Deadline, cancelamento ou parada | Falha causal atual, sem dados tardios. |

## Binding de referências da página

Uma página `found` observa `focused_source` pelo leitor nativo de referências,
com outgoing do slug literal e page_size 1. Página e metadata usam a mesma
Original e admissão Knowledge; o servidor exige igualdade de `KnowledgeGeneration`
do mesmo corpus, branch e versão antes de serializar. Isso não compara gerações
de Task, Memory, Raw ou embeddings. Uma alteração concorrente do corpus não
mistura body de uma geração com namespace/witness de outra.

`focused_source` conserva os seis campos do SDK: `slug`, `title`, `source_path`,
`compilation_digest`, `namespace` e `witness`. Zero arestas com projeção presente
mantém namespace e witness. Projeção ausente mantém metadata com namespace/witness
null e continua incompleta: seleção PROJECT, título, slug e body não fornecem
um namespace substituto. Heads, NoCorpus, PageAbsent e PublishedEmpty devolvem
focused_source null, preservando seus estados nativos.

O envelope é uma observação para navegação, sem autoridade. O cliente conserva
SourceKey, seleção e slug e usa somente namespace observado para construir o
foco de referências. `POST /api/records/references` e a próxima leitura de página
fazem sua própria admissão live. Knowledge importado usa o mesmo serializer
borrowed, omite seleção PROJECT e mantém page/references no mesmo reader/pin
retido. Artifact/version/alias identificam a fonte física; não substituem o
namespace do payload.

O consumidor mínimo deste recorte é Page HTTP → widget de referências em
`tsk-51f2`. Heads não é BM25 e o binding não qualifica Search ou Ask.
Os consumidores Backend de busca e Ask têm implementações e provas próprias
nas seções abaixo; a integração frontend mantém seu escopo separado.

## Consumidor React PROJECT

`NativeProjectKnowledge`, em `WikiExplorerPage`, usa `api/nativeKnowledge.ts`
para os POSTs heads/page com seleção exata NativePath, ID e versão. Heads envia
`input:{}`; page envia o slug literal. O cliente compara a seleção retornada
com a solicitada. O helper de URL mantém `selection` e `page`, e a chave física
do escopo recria o leitor quando a seleção muda. AbortSignal, limpeza do estado
e comparação do slug impedem que respostas tardias substituam a página atual.

A biblioteca apresenta somente os heads reais — slug, title e updated_at —,
sem inventar WikiModule, contagens, tags ou source para esse resultado. O
documento conserva os 16 campos canônicos, body literal, metadata, digests,
recipe, staleness e a ordem/repetições de `resolved_wiki_targets`. O renderer
genérico `MarkdownContent` recebe o body completo, sem preprocessing do body
Go. Targets registrados navegam literalmente para páginas da mesma seleção;
isso não qualifica a projeção completa de referências.
Copy Markdown copia o body literal completo. O controle Raw usa `aria-pressed`
para observar o modo de apresentação, sem modificar o documento.

A seção adicional Record links usa o compositor nativo de `tsk-51f2`, com
binding J9 de `source` físico e namespace/witness observados em Page PROJECT
ou importada, sem derivar foco de heads, título ou produtor. Conserva body,
metadata e referências raw; fontes adicionais são opt-in. Navegação e
readmissão do destino seguem o [contrato central de referências](rust_foreground_record_references.md).
ef75 COMPLETED 65, 51f2 COMPLETED 79 e Gantt já foram qualificados por seus
owners. db07 não declara FullNoLoss/compositor ou fechamento 17a7; é limite
de prova, sem pendências fictícias ou replay desses owners.

NoCorpus, publicação vazia, pageAbsent, loading e erro causal são estados
distintos; Retry conserva a seleção e o slug. O documento JSON completo usa
o codec comum `lossless-json 4.3.1`, preservando u64 literal como number seguro
ou bigint, sem arredondamento nem conversão para string.

O discovery PROJECT agora usa o módulo `knowledge` no store e
`POST /api/knowledge/contexts` com `{input:{}}`, dispensando Task e o projeto
global no App/header/Refresh. `current:null` é válido, sem primeira seleção
automática ou fallback Go na raiz. A troca limpa/aborta o estado anterior e
bloqueia respostas e negações tardias; Retry mantém a seleção explícita da URI.

Knowledge importado usa SourceKeyImported completo: `{kind:"imported_knowledge",artifact:{id,version,context_alias?}}`. A URI conserva `library=imported`, `source` como JSON e `page` como slug literal. Seleção importada inválida ou conflitante com PROJECT é rejeitada antes de IO. Os POSTs /api/knowledge/imported/contexts, /heads e /page não enviam seleção PROJECT. importedHeads envia o input fechado `{input:{source}}` e compara SourceKeyImported completo no retorno. A biblioteca importada navega pelos heads reais e abre o slug literal, preservando datas Native, geração/u64, registry_absent, publicação vazia, loading, erro causal, Retry e AbortSignal.

RegistryAbsent, publicação vazia, pageAbsent, found e erro HTTP/Retry são
estados distintos. O componente `NativeKnowledgeDocument` é compartilhado com
PROJECT: mantém os 16 campos, body completo, Raw, Copy Markdown e targets
repetidos. O escopo importado independe de contextos PROJECT; namespace e
Artifact/version/alias completos não são inferidos de título ou mount.

### Query nativa PROJECT e importada

A implementação R8 em `api/nativeKnowledge.ts` e
`NativeKnowledgeSearch.tsx` usa POST próprio: PROJECT envia
`{selection,input}` para /api/knowledge/search; importado envia
`{input:{source,search}}` para /api/knowledge/imported/search, sem PROJECT.
O cliente confere a seleção retornada ou SourceKeyImported completo antes de
aceitar o envelope. O controller captura binding/request e AbortSignal;
troca de escopo, nova intenção, refresh e cancelamento invalidam respostas
tardias. Retry repete somente a intenção capturada no mesmo binding.

Os controles oferecem fts, semantic e hybrid, e cosine/l2/dot. top_k conserva
NativeInteger sem arredondar u64; zero mantém os resultados acessíveis pela
paginação. page_size tem as opções 20/50/100 e preview é explícito.
More search results reapresenta o cursor opaco observado com os mesmos
parâmetros capturados. O viewer distingue no_corpus, registry_absent,
published_empty, no_hits, hits, partial e unavailable, preservando reason,
gerações e total. Preview e truncation são observações; lexical_score,
semantic_distance e rrf_score aparecem separadamente. Abrir resultado usa
o slug literal de focused_source. Estados parciais/indisponíveis não provocam
fallback FTS, nem se tornam um vazio saudável.

### Ask nativo PROJECT e stream

R9 em `nativeKnowledgeApi.ask` usa
`nativeRecordResponse('/knowledge/ask', {selection,input}, signal)` e o
`readAgentStream<T,P>` existente, com progress genérico e NativeJSON nos
frames. Erros HTTP e SSE com código mantêm sua causa; frames/progresso e
receipt preservam inteiros u64 sem conversão intermediária para float.
Não há novo pin ou parser paralelo.

`NativeKnowledgeAsk.tsx` é PROJECT neste recorte. O operador informa o
modelo literal obrigatório; não existe catálogo UI de modelos neste fluxo
nem modelo selecionado implicitamente. A escolha de evidência é exclusiva:
`selected_slugs`, inclusive o array vazio explícito, ou
`retrieval:{mode,top_k,metric?}`. Seleção vazia não vira retrieval.
FTS omite metric; semantic/hybrid usam a métrica observada no controle.
A UI exige limite positivo para retrieval e preserva NativeInteger.

Progress pode conter estruturas heterogêneas e fica separado do resultado.
Answer é exibida somente quando status é answer e
receipt.completion.answer é string. provided_slugs e receipt continuam
observações da execução, não prova de citação ou uso pelo modelo.
AbortSignal, epoch/request, binding completo, unmount e refresh descartam
progress/resultados tardios; cancel limpa a execução visível e Retry mantém
o input capturado no mesmo binding. Falhas, no_corpus, publicação vazia ou
negação não recebem answer inventada.

### Evidência atual e integração pendente

Query01 passou 20 casos em quatro arquivos, 4,13 s, antes da correção do ref;
type02 e lint02 passaram. A primeira rodada AskQuery01 teve 30 PASS/1 FAIL em
26,45 s: o teste disparava change num controle Radix. A correção usa teclado
ArrowDown e a opção real, sem substituir o seletor por mock.
AskQuery02 passou 31 casos em seis arquivos em 4,82 s, com exit 0:
API 8, stream 6, Ask 4, Query 4, Imported 5 e PROJECT 4.
TypeAsk01 noEmit saiu 0; LintAsk01 saiu 1 com log vazio e permanece uma falha,
sem diagnóstico inventado. LintAsk02 --max-warnings 0 saiu 0, com stdout vazio.
As rodadas se sobrepõem e não se somam ao baseline histórico.

### Catálogo nativo, rotas e renderer R10

WikiContextsPage é somente importado e reutiliza ImportedKnowledge.
importedKnowledgeLocation unifica a decisão de catálogo/explorer no App,
chrome e refresh sem PROJECT. ContextKnowledgeExplorer, preprocessContent,
WikiTree sem callers e api/wiki.ts com GET Go/AISearchResponse foram removidos.
As rotas antigas de App moduleId/wiki também foram retiradas; não há
compatibilidade ou fallback Go. O packet R10 foi revisado antes desse Source,
após duas respostas schedulerbusy e a terceira tentativa bem-sucedida.

O preview de imagem usa MarkdownImagePreview e estado local de MarkdownContent,
com interação por teclado; handlers globais do lightbox foram retirados.
O helper compactWikiLabel foi movido intacto para o utilitário UI existente
para satisfazer FastRefresh. LiveEvidence recebeu apenas ajuste de import,
sem mudança de comportamento.

A rodada native-catalog01 passou 60 testes em 11 arquivos, 16,89 s, exit 0.
TSC noEmit saiu 0. O warning de div dentro de p no renderer foi corrigido
com span; o rerun afetado Markdown passou dois casos em 1,76 s sem esse warning.
Lint inicial saiu 1 por warning do helper compacto. A segunda invocação
incluiu LiveEvidence e saiu 1 por dois warnings herdados de effects
(onChange/requests.current); eles não foram alterados fora do escopo
comportamental. O lint final dos arquivos próprios passou --max-warnings 0, exit 0, conforme
native-knowledge-catalog-lint-owned.log; a falha ampla herdada fica preservada.
Não se somam os 60 casos aos 31 históricos sobrepostos.

O novo node scripts/build-rust-ui.mjs saiu 0: tsc-b, Vite 6.4.2 com 3.793 módulos
em 4,93 s e cópia dos assets; dist reportou 2610,73 kB. O log dessa rodada está
sendo salvo pelo owner; timings/bytes anteriores permanecem históricos.
A fixture autorizada está limitada
a foreground_tasks.rs/hosting/probe para o cenário Web CRUD e Ask por
start_and_publish público e ambiente Codex controlado. As demais fixtures de
leitura/negação permanecem; somente nesse cenário o preflight HTTP de refs é
pulado para evitar replay. Compile05 passou em 2 min 00 s, com locked/offline e j4. A jornada real
permanece parcial conforme os resultados abaixo. Não houve novo pin, manifest ou mudança Rust de produção.

### Hosting e Browser db07: prova parcial

Host01 falhou em 12,04 s no SDK reference_document_map; a fixture passou a
usar ReferenceFields reais dos 21 documentos. Host02 falhou em 17,15 s com
AdmissionDenied porque os projetos extras não estavam capturados em
trusted_projects; o Seed foi antecipado para antes de Auth. Essas falhas e
correções de fixture não são produto qualificado por si.

A prova IAB separada observou Knowledge FTS com 22 hits, páginas 20+2;
Ask empty_selection e answer literal Observed browser fixture answer,
requested_model codex e provided_slugs Foreground wiki. Cancel question
mostrou request stopped sem answer anterior. Isso qualifica essas observações
parciais do cenário controlado, não identidade independente do modelo nem
join do processo pelo cancel.

No importado, catálogo/heads/documento com metadata/reload e FTS com um hit
foram observados. Semantic l2 retornou unavailable/model_unavailable.
Hybrid não foi qualificado: o host finito 03 encerrou antes da resposta e a UI
mostrou Failed to fetch. Não se infere resultado híbrido ou causa SDK desse
erro de transporte. O Browser global db07 permanece sem PASS; o receipt
separa unidades, compilação, shipping, IAB parcial, falhas e pendências.

### Rodada Host04 e correção Memory R11

Compile06 passou em 16,12 s. Após seed, a fixture usa uma publicação FTS SDK
real para PROJECT/USER na fixture histórica; esse seed não qualifica
initialize-writer de produção e foi retirado em R12. Host04 passou um teste em
351,41 s incluindo espera por controle. Browser04 manteve os três primeiros
estágios positivos, mas falhou no timeout de 20 s de create; Memory search 200
foi observado. O IAB confirmou create/update e history 1/2 nesse host.
A remoção não enviava POST: window.prompt não é suportado no IAB.

A Source UI agora usa MemoryRemovalForm por ModalPortal/work-dialog, com
registro, título, revisão e MemoryReadScope capturados, razão literal
obrigatória, cancel/Escape sem I/O antes da confirmação e invalidação por
escopo/foco/revisão/deleted. O erro tardio RemoveA→B é fenced; idempotência e
receipts confirmed/uncertain são preservados. Unit01 teve 37 PASS/1 FAIL
em 38 casos/29,25 s; corrigida a cadeia de revisão da fixture, MemoryPage
passou 21 casos/6,93 s. Os 17 casos dos outros três arquivos continuam provas
inalteradas da primeira rodada, sem somar overlaps. Type noEmit e os dois
lints afetados saíram 0. O Browser R11 ainda estava pendente nessa rodada; o fluxo Memory08 posterior
foi qualificado independentemente.
O runner usa 60 s só para CRUD e confirma o diálogo React com elapsed/status;
nenhum timeout de produção foi alterado. O
[contrato frontend Memory](rust_foreground_memory.md#remoção-por-diálogo-r11)
descreve a jornada.

A revisão Source↔docs corrigiu a closure de receipt conhecido: após guard
de escopo, acceptMutation encerra completed/confirmed antes do guard de foco.
RemoveA tardio não publica receipt sobre B e conserva settlement; o erro
continua fenced por foco. O novo teste mantém diálogo B e intento correto:
MemoryPage03 passou 22 casos/7,18 s e lint03 saiu 0, sem somar reruns.

Build03 passou tsc-b/Vite/cópia, 3.793 módulos/4,47 s. O index da rodada histórica Build03 tem
2.612.285 bytes, SHA-256
f1eb9de1a8de514385c512398f4fd41c282ba7efc0c885cfc430e0b564c46efc,
igual no dist/shipped. Build02 e Compile07 são históricos; bin07 não embute
o hash f1eb atual e não qualifica estes assets em runtime. Host05/Browser05 executaram depois; o novo R13 e seus limites estão abaixo.
R12 foi implementada na fixture/runner sem mudar Source UI ou bundle f1eb.
hosting.rs não tem mais MemorySeed::CreateWithLexicalIndex/create_lexical_index
nem bool USER; o cenário USER db07 usa Option<MemoryId>None sem as duas
escritas seed, preservando incoming_user e os demais References/cenários.
Preflight USER list observa no_corpus/scope real. O runner implementa
PROJECT extra/USER noCorpus→UI create→FTS200 com hit literal após create e
update, histórico e remoção React; bodies[mark:] verifica seleção exata.

py_compile/rustfmt/diff passaram. Compile08 usou offline/locked/j4 no target/profile existentes; a rodada
Host05 posterior não qualifica a nova Source UI R13.
Root confirmou Source produtora/fixtures 67e7 coesas Data3args; a relação
agora é uses_api. Não se aguarda COM/status 67e7 ou futuro AppMemoryIndex:
a jornada Browser contribui ao produtor e o gate anterior criava ciclo.
Disco 7,2 GiB é somente observação; o incidente SIGBUS antigo está resolvido.
Aquela rodada mantinha a matriz real pendente; os FAILs são preservados e
a qualificação final por camadas R17 aparece abaixo.

### R13 e rodada Host05: runtime ainda parcial

R13 implementa not_applied JSON/headers e index_publication opcional com
versão inteira/recovery real separada do CRUD; headers só preservam
index_sdk_version conhecido, sem generation/recovery inventados.
completed|confirmed fecham intento/chave/navegação; not_applied preserva
draft/confirmação/rota/chave e nunca faz retry automático. O notice com
Index SDK version e receipt completo também aparece dentro de Create/Edit.
Unit01 teve 33 PASS/2 FAIL/9,13 s por matcher inexistente; Unit02 passou 36 em
8,19s (11 API/25 page). Os problemas Type01 foram corrigidos somente em
tipos; build04 tsc-b final e lint02 de quatro arquivos saíram 0. A falha inicial
permanece; Unit02 não muda de contagem.

Host05 passou 273,77 s/shutdown/reopen; shipping f1eb passou 303 ms.
Browser05 falhou no callback reentrante Task latest, StopIteration apesar de
GET 200; o runner passou a enfileirar observação. IAB05 real qualificou somente
a sequência parcial USER NoCorpus→create completed→FTS 1 hit literal→cancel de
remoção sem excluir→confirmação com razão→reload deleted rev2/history.
Isso não passa matriz global, zero I/O automático ou a UI R13 nova.
Root DTO 4 PASS/0,05 s é prova produtora distinta de runtime React.
Build04 passou Vite 3.793 módulos/19,43 s e copiou index 2.612.899 bytes,
SHA-256 b4e594e1d03a0690638901cfaf7200e539f51613dde223279f38b5c83037f39a,
igual dist/shipped. Compile09 passou 3 min 28 s e embute esses assets R13.
Host06 falhou antes do probe em 21,21 s no startup hosting.rs:1469:
ForegroundFailureInvalidData/ProfileDecodeFailure stock/plsql.yaml,
Decode span 35365..35365, schema_path None e retained_owner true.
Browser06 foi prerequisiteFAIL, zero Browser e nenhum IAB atual.
Root encaminhou AST/C8 e não haverá novo Host até Source de perfil coesa,
sem alterar AST/Runtime ou contornar bootstrap. Unit02 36 PASS/8,19 s,
lint02 exit 0, build04 e Compile09 mantêm seus PASS próprios, sem runtime PASS.
Os checks planejados de FTS com termo exclusivo após revisão e exclusão
corrente/retained history ainda não foram executados; sem novo Source UI/asset.

### IAB07 e escopo focal R15

Compile10 PASS 32,04 s e Host07 PASS 335,27 s, com b4 servido, sucederam a correção
C8. Browser07 não diagnostica CRUD: SIGINT interrompeu o coletor Task com
Response.json pendente e cleanup sem running event loop, sem HTTP inventado.
IAB07 registra USER real: noCorpus/create completed/FTS literal 1/update termo
exclusivo revisão 2/removeReact razão/reload deleted revisão 3/FTS corrente 0
versus retained 1. A prova é parcial; zero PROJECT traffic não foi afirmado.

O receipt create anterior aparecia ao abrir novo Edit. Source agora limpa
só setMutation nas novas entradas Create/Edit, preservando key/receipt do
intento em andamento. Unit03 teve 37 PASS/1 FAIL/8,83 s: callback Edit ainda
não limpo. Handler corrigido; Unit04 passou 38/2 arquivos/9,07 s (11 API+27 page).
Lint03 e lint04 final saíram 0. Build05 exit 0 tsc-b/Vite
3.793 módulos/6,77 s, index 2.612.921 bytes, SHA-256
9678a314f4b42c5c8d363cd0a56f6e7389bbf1e35768e771ab96dec3b6fa48f1,
igual dist/shipped. Compile11 exit 0 em 14,98 s. Browser08 já preserva Memory qualificada;
central current_runtime mantém Knowledge pendente, com teardown concluído.

R15 está salva antes de Source e o runner focal coleta só JSON POST
Memory/USER/Knowledge com snapshot e qualification.json a cada passed antes
do teardown, preservando prova Memory se Knowledge falhar depois.
Reusa Browser03/04 para Task e módulos próprios e IAB03 para Knowledge FTS,
paginação, Ask e navegação importada; retira replays Task/Session/CRUD/Ask/FTS paging.
Permanecem a primeira Memory PROJECT, USER sem I/O PROJECT e viewport estreito,
Mirror, Knowledge hybrid, busca importada semantic/hybrid e estados de
corpus/página, heads→page importado sem I/O PROJECT, cancelamento de query
aceita sem resposta tardia e layout estreito/negação.
Nada disso recebe PASS sem execução da nova matriz.

### Rodada08: Memory independente, Knowledge pendente

Browser08 no bundle 9678 qualificou PROJECT first create/FTS/update exclusivo/
delete/history/current0-retained1 em 7.540 ms; USER mesma jornada em 7.696 ms
com zero PROJECT/Task I/O; Mirror NativePath/reload em 2.763 ms. Remoção React
narrow 390×844 não teve overflow e Tab moveu razão→Cancel. IAB08 confirmou
counts 0 de receipt anterior em novo Edit/Create. Esses PASS independentes
de Memory sobrevivem à falha posterior do runner, sem replay de owners.

O runner global aguardou 20 s pelo HTTP de Knowledge hybrid/dot e falhou;
não foi um erro de seletor. IAB08 viu o resultado já partial 22/
model_unavailable até 93,434 s depois do click. Isso é somente instante da
observação: conclusão e latência não foram medidas. O coordenador rastreou
default produtivo 120 s; confirmou fixture 120000 ms e ajustou só a espera
do runner para 130000 ms na rodada09. Não se marca hybrid ou a matriz Knowledge como PASS.

Ainda faltam Knowledge hybrid, import semantic/hybrid, estados corpus/page,
heads→page sem I/O PROJECT, cancelamento aceito com resposta tardia oculta
e os checks narrow/denial restantes. Host08 terminou exit 0, 1 PASS/0 FAIL,
8 filtered, finished 693,57 s; .done saiu antes de 900 s e teardown está concluído.
Runner08 terminou exit 1 pelo timeout HTTP 20 s conhecido, preservando a falha
sem invalidar Memory. Lint04, py_compile e focus-help terminaram exit 0.
Memory/Task/Gantt/Refs não serão reconstruídos e FullNoLoss permanece fora
desta declaração.

### Knowledge09: hybrid aprovado e gap de estado

A rodada focal 09, bundle 9678/binário Compile11, passou PROJECT hybrid/dot:
HTTP 200 partial/model_unavailable, 49.494 ms click→JSON completo e 58.274 ms
para o estágio inteiro. Actual shipping 131 ms e signin 1.051 ms são separados.
A medição query_http completa agora existe; a observação IAB08 até 93,434 s
continua histórica e não era latência medida.

O runner global saiu 1 na linha 310 esperando no_corpus, mas havia uma
publicação vazia publication_version 1; a primeira published_empty passou.
Root e owner Knowledge confirmaram o
[contrato Knowledge-only/NoM](../architecture/rust-knowledge-foreground.md#knowledge-no-foreground-rust):
watcher publica geração 1 vazia com docs ausente/vazio, comprovado por
r9-empty-foreground-09 (uma função/dois cenários/11,11 s). Logo,
PublishedEmpty09 é correto, não hipótese de defeito. Knowledge NoCorpus
não foi observado no Browser; R17 resolve a cobertura por camadas abaixo,
sem tratá-lo como gap funcional. Host09 passou exit 0, 1 PASS/0 FAIL/123,03 s, done/cancel/reopen.

Só a espera do runner mudou: 130000 ms = fixture 120000 ms + 10 s, nenhum limite
produtivo. Host10 segue ativo em --records-focus knowledge-imported para
imported/cancel/narrow/zeroIO; a coleta 10 depois falhou na entrada UI,
conforme R16 abaixo, preservando Memory08 e PROJECT hybrid09. A2/T2 históricos foram supersedidos para R15 pelo owner;
não há pendências produtoras fictícias ou necessidade de repetir owners.
Knowledge não está completo: estados e os faltantes importados permanecem.

### R16: rota fixa implementada, execução pendente

Host10 passou exit 0, 1 PASS/66,33 s. Runner10 falhou no signin linha 245 antes
das APIs de domínio, porque a entrada /knowledge/contexts devolvia 404.
A rota React existia; faltava a rota fixa Foreground. Root cedeu Source
foreground_ui.rs e teste; o mesmo index agora cobre essa entrada, com
GET/HEAD/query/404 subrota no teste existente. Route01 saiu 101 após 102,085 s
por os28/No space left on device escrevendo
target/debug/deps/liblbug-6a3a53fb057dfaeb.rlib. O libtest não executou nem
chegou ao assert; falha de ambiente, não da asserção de rota. Compile12 não
iniciou, pois o orquestrador para no erro; runtime11 também não iniciou.
df 109M livres/100% motivou aviso ao Root para recuperação compartilhada,
sem retry/clean do consumer. Naquela tentativa R16 estava implementada mas
não qualificada; o retry autorizado abaixo passou a rota unitária.
py_compile/diff/rustfmt check saíram 0, sem PASS de rota/runtime.

R16 foi salva em Task 110 antes de Source. O runner agora afirma zero I/O
global projects/project-catalog/project contexts e user-memory no import-only,
e somente leitura USER zeroDiscovery sem repetir CRUD08. Após release,
observa abort terminal e tenta late delivery do HTTP 200 real retido pelo
padrão AST. Nenhum resultado desses checks é antecipado. Bundle 9678
permanece; Memory08/PROJECT hybrid09 e owners qualificados não são repetidos.

### Route02 e binário correto para Runtime12

Root recuperou 18 GB somente de executáveis antigos e autorizou retry.
native-contexts-route-02 passou 1/0/runtime 0,10 s, compile+test 78,788 s,
com GET/HEAD/query/404. Compile12 exit 0/9,368 s no comando, 9,17 s reportados
pelo compilador, produziu foreground_tasks-01664d47aa20fd05.

Host11 exit 0/1 PASS/67,42 s usou hardcoded foreground_tasks-6286b13e39a1667a
por erro do harness; runner11 falhou signin/stale404 e não executa R16.
browser-11/qualification.json preserva harness_error e os paths actual/compiled.
Host12 usa o executável extraído do log Compile12 e gravará SHA ao terminar,
sem recompile. Runtime12 e imported/cancel/late/zeroIO continuam sem resultado;
a prova unitária da rota não é convertida em PASS Browser.

### Browser13: importado qualificado, controles ainda focais

Bundle 9678 e binário Compile12 SHA-256
1330be07d996d72607c29038dc2ee9e0d69809c926a5a24188071c3c9ee84e31
serviram /knowledge/contexts/reload e import inventory→heads→page/reload:
PASS 32.892 ms, SourceKey exato em URI/HTTP e zero PROJECT/Task/global discovery
no intervalo. Semantic/l2 HTTP 200 unavailable/model_unavailable levou 6.569 ms,
hybrid/dot HTTP 200 partial/model_unavailable 14.789 ms click→JSON completo.
PROJECT slug literal ausente passou em 18.520 ms. PublishedEmpty inicial com
HTTP 200/seleção exata passou em 1.191 ms; startup não foi congelado.
Knowledge NoCorpus é não-Browser, coberto por camadas em R17 abaixo.

Runtime14/15 executaram knowledge-controls com matcher exact=False como AST;
os resultados e seus limites finais estão abaixo.
Reusa imports/queries/absent já qualificados, Memory08/PROJECT hybrid09 e
owners, sem recompile nem replay CRUD/Task/Gantt/refs.

### Qualificação final R17: jornadas independentes e limites por camada

O mesmo bundle 9678/index 2.612.921 bytes e binário Compile12 SHA-256
1330be07d996d72607c29038dc2ee9e0d69809c926a5a24188071c3c9ee84e31
serviram as rodadas 14/15. Browser14 passou cancel SEARCH aceito, issuer
entered/release/ERR_ABORTED (9.202 ms); reply importado HTTP 200 real/hits 1
retido→Cancel terminal→tentativa fulfill_after_cancel→sem resultados
(7.540 ms). Não se afirma que JS recebeu a resposta tardia. USER read 200
usou apenas /api/user-memory/list em 801 ms, zero PROJECT/Task/global discovery.

| Contrato | Camada que o prova | Limite explícito |
| --- | --- | --- |
| Knowledge NoCorpus, vazio e página ausente | Wire nativeKnowledge.test.ts103; React WikiExplorerNative.test.tsx47; backend HTTP real c135 COM20 K2/KT1 e Source knowledge.rs176/251–259/342–357 | NoCorpus não observado no Browser; cobertura funcional por camadas, sem gap funcional. |
| Heads PROJECT cancel/context change | Unit WikiExplorerNative.test.tsx63, abort do oldHeadsSignal73; Source WikiExplorerPage47–55/cleanup73 | Late74–75 é PAGE; não houve prova HTTP cancel/late HEADS. |
| Heads importados abort/late R18 | Unit ImportedKnowledge.test.tsx66–91: importedHeads deferido, sinal antigo abortado após trocar a tupla, heads Mirror aguardados, HEADS e PAGE antigos resolvidos depois | Lista atual preservada; título/slug antigos ausentes. Prova unitária, sem cancel/late HEADS por HTTP. |
| Search cancel/late e zero discovery | Browser14 com resposta HTTP 200 real retida, tentativa de entrega após cancel e USER read | SEARCH; não extrapola para HEADS nem afirma entrega JS. |
| Narrow/teclado/logout/negação | Browser15 real, importedPage401 e heading anterior ausente | Escopo db07, sem FullNoLoss/compositor ou fechamento de outros owners. |

Task tsk-c135 foi lida em COMPLETED 20; K2 chk-b993f80ad491 e KT1
chk-ea51ce9ccd3f estão PASS. Ela sustenta a execução histórica HTTP08,
13 Knowledge em 7,99 s. Source atual confirma mirror NoCorpus heads/page e
a distinção published_empty. Logs .build históricos estão ausentes;
não se alega releitura deles ou rerun por esta transcrição.
As suites retidas catalog01 60 PASS/16,89 s e AskQuery02 31 PASS/4,82 s seguem
válidas sem somar sobreposição.

R17 foi salva em Task 125; T2 superseded 126 recebeu novo
chk-ed34612e1fa3 pelo owner, sem writes desta transcrição.
Runner futuro usa PublishedEmpty+slugAbsent, sem expectativa transitória
NoCorpus, e coverage_by_layer declara NoCorpus não-Browser/heads unit.
A qualificação é somente escopo db07 por camadas. Main ainda registra
checks/unflag/completion. Não se concluem FullNoLoss/compositor/17a7/69f2/e738
nem migração global; nenhum owner será repetido para fabricar prova.

## Implementação e qualificação

O adapter privado está em `crates/graphit-app/src/foreground_knowledge.rs`;
rotas e admissão em `browser_auth/task.rs` e `task_server/browser.rs`. O reader
Data é `knowledge_index.rs`, com seleção de página em `knowledge_snapshot.rs`.
As fixtures HTTP usam admissions Auth, publicação Lance e reabertura reais.

A qualificação executou treze testes HTTP Knowledge e as doze regressões Memory
afetadas pelo codec e pela correção do envelope NativePath, mais um teste de
serializer: 26 testes distintos aprovados, sem ignored. O envelope passa a usar
a seleção retida e admitida, conservando seus bytes em vez de reconstruir o path
pela captura física. O Clippy de App lib/testes passou com
`--no-deps -- -D warnings`; formato, whitespace, links e referências da
documentação foram conferidos.

Os resultados e as falhas anteriores estão em
`.build/rust-native-knowledge-browser/qualification.json`. Task/References
conservam seus parsers, tuples e outputs e não tiveram suas suites repetidas.
A unidade cobre o browser PROJECT e suas fronteiras afetadas; USER/imports,
projeções de referências do produtor, UI React, outros sistemas operacionais e a
migração global têm suas próprias entregas.

## Decoder fechado em uma passagem

A limpeza 4e50 retirou a árvore JSON intermediária e o segundo decode do mesmo
corpo. Cinco testes focais dos consumidores verificaram campos desconhecidos,
duplicatas escapadas, null/tipos inválidos, limites reais de bytes/texto/units,
IDs canônicos e NativePath literal sem perda de bytes. As doze regressões HTTP
Memory e treze Knowledge também passaram, incluindo permissões, corrupção,
cancelamento e limites de saída: 30 testes distintos desta unidade, sem ignored.
O pequeno QueryValue exigido pelo dispatcher existente permanece; o decoder não
cria um parser ou protocolo paralelo. Clippy App lib/testes passou com
`--no-deps -- -D warnings`; formato e documentação foram conferidos. Evidências ficam em
`.build/rust-fixed-browser-decoder/qualification.json`.

## Discovery próprio e Knowledge importado

`POST /api/knowledge/contexts` recebe `{input:{}}` e usa Knowledge/read por projeto, independente de Task. Retorna apenas tuplas admitidas e current null quando o contexto configurado foi negado.

Os leitores `/api/knowledge/imported/contexts` e `/api/knowledge/imported/page` usam a instalação e o Registry global existentes com authority Web explicitamente configurada; não recebem seleção PROJECT. O leitor importado reutiliza a view FULL16 desta página e revalida o mesmo reader após encoding. Os [contratos de referências e imports](rust_foreground_record_references.md) descrevem seus corpos, estados, limites e qualificação pendente. Um mount alias não altera o namespace persistido da referência.

## Ask no Browser

Em Linux, `POST /api/knowledge/ask` recebe a mesma seleção PROJECT exata das
leituras acima e `input: {query, selected_slugs, model}` ou a alternativa
exclusiva `{query, model, retrieval: {mode, top_k, metric?}}`. O input é fechado;
slugs, pergunta e modelo são dados literais, não comandos ou authority.
Cookie, Origin, access JWT local e uma decisão fresca `Knowledge/read` precedem
a abertura do fluxo. Negação de acesso e ausência do serviço configurado
continuam respostas HTTP de erro, sem lançar o provedor.

O executor compartilha o mesmo `Arc<AskAvailability>` e Host capturados por
`SourceServer::attach_ask`, com o binding Rust atual (por exemplo Codex, AGY ou Kiro V3), conforme a configuração
nativa. Não cria engine, client, lease CLI ou captura Runtime do usuário Web.
A admissão Web, os readers e a execução usam uma Original finita única.
Apenas o bearer necessário permanece durante a execução; o mutex das
credenciais da sessão é liberado antes de abrir o corpus ou executar o CLI.

O serviço prepara as páginas realmente selecionadas com o SDK documental.
`no_corpus`, `published_empty` e `empty_selection` permanecem estados distintos
sem executar o provedor. A seleção explícita, inclusive `[]`, nunca aciona
recuperação automática. A alternativa `retrieval` está descrita abaixo.

O HTTP usa SSE com eventos `progress` e `final`. Progress contém a projeção
nativa já usada pelo MCP; JSON numérico validado pelo decoder não passa por
coerção adicional. O canal retém no máximo um evento, aplicando backpressure.
Final contém o status nativo, geração observada, `provided_slugs` e receipt,
quando existentes. `provided_slugs` descreve páginas fornecidas ao provedor,
sem provar citações, utilização pelo modelo ou identidade de execução. O
cliente não deve tratar esses campos como concessão de leitura ou autorização
para repetir uma operação.

Sessão, admissão, parada e tupla ativa são rechecadas durante progresso e
entrega. Drop do body cancela a mesma Original. O trabalho real permanece no
JoinSet existente de TaskServer até join, antes do shutdown do Host.
Uma negação após execução retira a resposta do modelo e preserva o receipt
observado e sua custódia. O limite concreto de saída também é aplicado ao
final; primeiro tenta entregar a negação sem answer e, se ela exceder o limite,
entrega `response_limit`. Isso não repete a execução nem abandona o Process.

A instrução vem de
[knowledge-question.md](../../templates/agent-instructions/knowledge-question.md),
renderizada pelo MiniJinja strict existente, sem loader de arquivos. Pergunta
e páginas permanecem campos de dados do prompt, sem variáveis de template
ou interpolação de instruções dessas páginas.

Entradas produtivas:
[browser_auth/task.rs](../../crates/graphit-app/src/browser_auth/task.rs),
[task_server/browser.rs](../../crates/graphit-app/src/task_server/browser.rs),
[foreground_knowledge/ask.rs](../../crates/graphit-app/src/foreground_knowledge/ask.rs),
[source_server/knowledge_ask.rs](../../crates/graphit-app/src/source_server/knowledge_ask.rs)
e [knowledge_question.rs](../../crates/graphit-app/src/knowledge_question.rs).
As três novas fixtures HTTP estão em `browser_auth/tests/knowledge.rs`; sua
qualificação é independente dos 14 casos J9 e não qualifica o frontend.

### Ask com recuperação automática

`retrieval` aceita `mode: fts|semantic|hybrid` e `top_k` positivo, limitado pela
seleção real do prompt. `metric` pertence somente a semantic/hybrid; FTS com
metric é inválido. A mesma alternativa existe no tool MCP e no comando
`knowledge ask --retrieval hybrid --top-k 5`, exclusivo de `--slug`.

A pergunta alimenta Search sem previews. A paginação conserva apenas os slugs
selecionados e reutiliza o único vetor Query quando M está pronto. Os documentos
são lidos no mesmo pin por `read_selected_documents`, sem recaptura e sem Vec do
corpus inteiro. O preparo público conserva seus getters de documentos; ao
transferi-lo ao binding selecionado (por exemplo Codex, AGY ou Kiro V3), descarta bodies e summaries antes de
aguardar o subprocesso, mantendo prompt e metadados `KnowledgeHead`.

FTS não exige M. Híbrido sem M usa o lexical do mesmo pin como `partial`, sem
score semântico ou RRF inventado. Semântico sem M é `unavailable`. NoHits,
NoCorpus ou indisponibilidade sem documentos não iniciam o provedor. Falhas de
autorização, integridade e transporte não viram fallback. A projeção `retrieval`
conserva estado, gerações e slugs observados; páginas fornecidas continuam sem
promessa de citações verificadas. Ask automático não aceita contexto importado.

## Consumidores lexicais Knowledge

A busca lexical usa `mode: "fts"` explícito. Browser PROJECT recebe
`POST /api/knowledge/search` com `{selection, input}`; `input` contém
`query`, `mode`, `top_k` (zero mantém todos os resultados paginados),
`page_size` (padrão 20, de 1 a 100), `preview` (padrão false) e o
`cursor` opaco quando houver continuação. A seleção é NativePath, id e
versão literais, e cada requisição admite ReadKnowledge novamente.

Browser importado usa `POST /api/knowledge/imported/search` com
`{input: {source, search}}`. `source` é ImportedKnowledge com artifact
id/version/context_alias observados; não é seleção PROJECT. O reader
retém a admissão e o pin da Copy registrados, sem conceder PROJECT nem
consultar o Broker por permissão de um projeto de autoria.

O tool MCP `graphit_knowledge_search` usa `{search, project_dir?,
project_version?, source?}`. Sem `source`, seleciona PROJECT corrente ou
explícito. ImportedKnowledge usa somente `source` e rejeita seletores
PROJECT misturados. Runtime/Local é autenticado e a admissão importada
exige ReadKnowledge própria; registro e cursor não concedem acesso.

As respostas têm `source`, `focused_source` e `native`. PROJECT também
retorna sua seleção admitida. Cada hit tem `focused_source`,
`lexical_score` e preview opcional (`text`, `source`, range de bytes e
`truncated`). Namespace/witness ausentes permanecem null: esse foco é
incompleto para resolver referências, e não se fabrica um namespace
da seleção ou do alias. O preview é produzido pelo formatter enquanto
o hit empresta o batch do SDK; nenhum documento completo é retido na
página de resultados. O corpo completo continua no endpoint Page.

`native.state` distingue `no_corpus`, `registry_absent`, `published_empty`, `no_hits` e `hits`. Cursor inválido dá 400; mudança do pin dá 409. Native MCP retorna erro estruturado correspondente. Continuação reaplica autenticação, autorização, Original e a captura corrente. FTS funciona sem M; não é apresentado como modo híbrido. A prova Browser usa UserInfo indisponível e nenhum M, com preview UTF-8 e cursor/pin reais; o importado nega PROJECT e lê sua própria Copy registrada. O caso CLI usa o transporte `graphit mcp --stdio` a partir da instalação, sem alegar startup PROJECT. Provas do SDK e Ask explícito não são contadas como execução desses consumidores.

O conjunto de jobs do Source conserva a primeira JoinError real no próprio
owner, enquanto libera os jobs já concluídos. A busca seguinte mantém seu
resultado e Original independentes; erro de worker anterior não altera o
estado do corpus. Encerramento aguarda todos os jobs e devolve a causa tipada
no resultado do serviço, inclusive quando um request anterior já terminou.
Esse estado não é contabilidade de recursos nem um supervisor adicional.

### Busca semântica e híbrida PROJECT

A implementação App também recebe `mode: semantic|hybrid` e `metric:
cosine|l2|dot` nos mesmos endpoints e tool. O ingresso Web preserva o
principal verificado e a admissão ReadKnowledge; captura uma lease local
Embedding para esse próprio caller. Native conserva sua identidade nativa.
Ambos usam o cliente, processo e configuração M compartilhados do Foreground.

A consulta captura Source e NN juntos, percorre documentos até EOF e conserva
apenas slug/fingerprint para verificar a projeção corrente. Readiness precede
qualquer conexão: somente Ready envia uma Query RPC. Ranking, filtros,
paginação e combinação híbrida pertencem ao SDK Knowledge; o App não combina
duas capturas. A resposta mantém as gerações Source/NN e scores observados:
`lexical_score`, `semantic_distance` e `rrf_score` aparecem somente quando
produzidos pelo modo correspondente.

M ausente ou Embedding desativado para a seleção não cria uma identidade de
modelo. A leitura lexical reutiliza a mesma captura Source/NN: híbrido retorna
`partial` com hits lexicais e `reason: model_unavailable`, sem distância ou
RRF fabricados; semântico retorna `unavailable` sem hits. Source vazio e
Source ausente continuam distintos. O gate usa o mesmo HookPolicy capturado
no startup, preservando command/environment, e relê a configuração do projeto
selecionado sob o Original da operação. O lock de attachment só clona os
recursos antes de IO. Erros de autorização, integridade ou transporte não
viram degradação silenciosa. NN não pronto não chama M, mesmo se o endpoint
estiver inacessível.

## Exportação pela CLI

`knowledge export --output PATH` conserva o pacote Native como formato default.
`knowledge export --format markdown|okf|obsidian --output DIRECTORY` encaminha a seleção à
mesma tool `graphit_knowledge_export`, com PROJECT físico e versão capturados
pelo Main. `--project-dir` aceita outro checkout selecionável pelo serviço;
`--project-version` conserva a versão explícita. Todos os formatos exigem a
admissão ReadKnowledge e o efeito PublishedFilesystem na mesma operação.

Os três formatos de texto entregam `index.md` e `pages/p-<SHA256 do slug>.md`, com links reais do
índice, metadata YAML nativa e corpo autoral como sufixo da seção Content.
Markdown e OKF conservam o corpo literal. Obsidian traduz wikilinks locais
observados para os arquivos exportados, preservando âncora, display implícito ou
label explícito; links sem destino observado e os contidos em código/HTML
permanecem literais. A normalização aceita referências percent-encoded aos slugs
canônicos existentes. Markdown conserva os 15 campos nativos no cabeçalho. OKF e Obsidian declaram
`okf_version: "0.2"` no índice e preservam esses campos na extensão `graphit`
das páginas. O cabeçalho inclui tipo, id, título, descrição observada,
`sources[].resource` e `generated.by: process:<marca>-knowledge-wiki`. A marca
vem da instalação; `generated.at` só usa `updated_at` publicado quando presente,
e duração de staleness não vira uma data absoluta inventada. `source_path` é a
proveniência lógica publicada (por exemplo, uma chave `knowledge-source/...`),
não um caminho de checkout a ser decodificado para leitura. Os links gerados
apontam para os arquivos publicados; links autorais permanecem literais.
O JSON de sucesso conserva `format`, `output`, `generation`, `fingerprint`,
`communities` e `cleanup`. O diretório precisa estar ausente e seu parent existir,
inclusive para output fora do PROJECT ou da instalação. A renomeação exclusiva
preserva um destino ocupado e o erro conserva `io_error_kind: "AlreadyExists"`,
`output`, `publication: "unconfirmed"` e o resultado real de cleanup; outros erros
IO não são convertidos em alegação de rollback. A publicação só ocorre depois do EOF
validado da captura Source-only. Formato inválido falha na sintaxe da CLI;
versão PROJECT inexistente falha sem publicar arquivos. Os detalhes de
[navegação e metadata](../architecture/rust-knowledge-markdown-export.md) são
compartilhados com o produtor SDK.

Para Knowledge instalado, `--artifact-id` e `--artifact-version` selecionam o
artefato exato e `--context-alias` conserva o contexto opcional. Esses flags
excluem `--project-dir` e uma versão PROJECT explícita. Native, Markdown, OKF e Obsidian usam a
mesma tool e a admissão real ImportedKnowledgeRead da instalação, além de
PublishedFilesystem do mesmo caller e Original. O mesmo estágio de texto lê o mesmo
pin Source até EOF, sem PROJECT ou NN, e rejeita output dentro do payload
readonly antes do primeiro spool. Uma versão ausente retorna `registry_absent`,
sem arquivos e sem fallback para o projeto ativo. Native reexporta o pacote
físico e pode ser importado e registrado por LocalLink novamente; importação
física isolada não cria registro nem autorização.

A entrada prepara um único `CallToolRequestParams` com destino e seleção
validados antes de iniciar ou manter o daemon. A chamada consome esse mesmo
payload pela conexão Source autenticada, sem interpretar os argumentos novamente.

## Busca pela CLI

No Main Rust, `knowledge search` encaminha a consulta à mesma tool autenticada
`graphit_knowledge_search`. Não abre outro store ou executa outra inferência.
O projeto e sua versão são os resolvidos pela invocação; `--project-dir` permite
selecionar explicitamente um checkout retido pelo serviço.

```text
graphit knowledge search literal --mode fts --page-size 20 --preview
graphit knowledge search literal --artifact-id app-knowledge --artifact-version release-1 --context-alias wiki
```

A seleção de artefato é independente de PROJECT e funciona fora de um checkout.
Exige ID e versão juntos, admite alias opcional e exclui `--project-dir` e uma
versão PROJECT explícita. O servidor mantém a autorização própria do contexto.

`--mode` aceita `fts` (default), `semantic` ou `hybrid`; `--metric` aceita
`cosine`, `l2` ou `dot`. `--top-k`, `--page-size` e `--preview` conservam os
contratos do serviço: zero em top-k não significa uma seleção vazia, e o tamanho
da página vai de 1 a 100. O CLI imprime o JSON original da resposta, incluindo
namespace, estado, geração, scores e cursor. Inteiros nativos não passam por
conversão em ponto flutuante.

Para continuar, passe em `--cursor` o objeto JSON exato retornado pela consulta,
conservando a seleção e os demais parâmetros. Não interprete nem reconstrua os
campos desse cursor. Erros estruturados do serviço conservam seu código e fazem
o comando falhar; não viram resultado vazio. Cancelamento e deadline encerram
a conexão cliente e permanecem erros, usando o mesmo contexto original.

## Busca semântica de Knowledge importado

Os ingressos Native MCP/CLI e HTTP `imported_knowledge_search` reutilizam o
mesmo consumidor em `foreground_knowledge/imported_search.rs`. A seleção usa
Registry, instalação e versão exata do artefato, com admissão própria de
Knowledge importado. Não exige nem fabrica um PROJECT do autor do pacote.
FTS continua no leitor Source-only, sem abrir NN ou consultar Models.

Semantic e Hybrid retêm uma única captura Source/NN e validam o cursor
documental até EOF. Cada lote descarta corpos e receitas após produzir os
fingerprints compactos necessários. A identidade esperada vem da configuração
atual do modelo; os metadados do pacote não substituem essa configuração.
Somente readiness Ready executa uma Query real no serviço local Models, com
lease concedida pela admissão importada e o mesmo Original. Não há inferência
de documentos nem abertura de outra geração para a consulta.

Se o serviço local não estiver anexado, o consumidor drena o mesmo cursor sem
inventar identidade: Semantic devolve indisponibilidade, enquanto Hybrid pode
devolver a parte lexical com as gerações Source/NN observadas. Ausência no
Registry, PublishedEmpty e NoHits permanecem estados distintos. Auth, pacote
inválido, alteração de seleção, rede, cancelamento e prazo continuam erros.
A codificação mantém scores separados, namespace real, cursor opaco e preview
do pin retido; um cursor nunca concede acesso.

Os dois consumidores passaram com CPU real: Native MCP na tentativa 03 e
HTTP Web na tentativa 06 (23,57 s de execução Web). Os casos usam Source/NN
produzidos pelo SDK, pacote físico exportado e importado e Registry com versão
exata. Verificam paginação, pins e namespace importados, scores, ausência no
Registry, estados sem modelo, cancelamento Native e autorização Web própria.
No Web, UserInfo indisponível não é chamado; retirar o provider invalida o acesso.

### Retomada semântica junto com delta

Um delta estrutural não substitui trabalho NN pendente de arquivos intactos.
Quando há recuperação de inferência ou mudança do modelo, depois do delta o
produtor admite também os slugs confirmados por metadata, sem ler ou parsear
novamente seus arquivos. A fila leva somente nomes; o consumidor deduplica por
job e reutiliza fingerprints compatíveis. Na mudança de espaço, todos os slugs
atuais são admitidos na reindexação normal. Falha Source conserva prioridade e
seu recibo real, inclusive se a passagem adicional falhar por Auth/cancelamento.
