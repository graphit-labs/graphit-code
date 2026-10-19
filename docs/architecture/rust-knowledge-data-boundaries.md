---
title: Fronteiras Knowledge, Data e Records na migração Rust
doc_type: architecture
references:
  - target: {type: task, id: tsk-c8e9}
    relation: relates_to
  - target: {type: task, id: tsk-a542}
    relation: specified_by
  - target: {type: task, id: tsk-7240}
    relation: relates_to
  - target: {type: task, id: tsk-d581}
    relation: specified_by
  - target: {type: task, id: tsk-1e53}
    relation: relates_to
  - target: {type: task, id: tsk-8218}
    relation: integrates
  - target: {type: task, id: tsk-72ce}
    relation: specified_by
  - target: {type: task, id: tsk-1953}
    relation: derived_from
  - target: {type: task, id: tsk-84d5}
    relation: specified_by
  - target: {type: task, id: tsk-a59d}
    relation: derived_from
  - target: {type: task, id: tsk-0a36}
    relation: derived_from
  - target: {type: task, id: tsk-b210}
    relation: relates_to
  - target: {type: task, id: tsk-fa72}
    relation: derived_from
  - target: {type: task, id: tsk-ed14}
    relation: derived_from
  - target: {type: task, id: tsk-dece}
    relation: derived_from
  - target: {type: task, id: tsk-8fcc}
    relation: relates_to
  - target: {type: knowledge, id: Wiki_Module_Specification}
    relation: supports
  - target: {type: knowledge, id: Persisted_record_relationships}
    relation: supports
  - target: {type: task, id: tsk-185c}
    relation: refined_by
---

# Fronteiras Knowledge, Data e Records na migração Rust

## Produtor Data de primeira publicação e leitura nativa

`crates/graphit-data/src/knowledge_index.rs` implementa a primeira publicação e a
leitura completa do índice `runtime_root/knowledge/index.lance`. A qualificação
SDK13 inicial usou um workspace privado. O namespace público Main é
`graphit_data::knowledge_index`, com Auth e Tokio como dependências de produção
e Hub nas fixtures de desenvolvimento. A ativação Main `tsk-8218` foi qualificada
em Linux com seis testes Knowledge, oito testes Wiki da composição atual, três
doctests de H, Clippy estrito e formatação. Dois dos casos Wiki pertencem à
leitura de heads de `tsk-a542`; sua entrega mantém critérios próprios. O caller
Knowledge/M1b permanece uma entrega própria. As APIs não entregam P-S/P-D,
search, lifecycle ou um store Task/Session/Memory. As propostas nas seções abaixo
continuam restritas aos respectivos produtores.

O publisher recebe `PreparedKnowledgeDocumentRows` do H real de Data, o mesmo
`Arc<SnapshotContext>` e `ProjectWriteAdmission` de operação Knowledge. Primeiro
valida classe/request, payload e limites; depois adquire o ArtifactWriter original
e cria exclusivamente o child `knowledge`. A espera pelo lock síncrono do Core
é executada em `spawn_blocking`, sem bloquear o executor que avança a publicação
concorrente. Auth é rechecado após a aquisição, antes do nascimento. Child
existente nunca é adotado.
O nascimento usa `create_record_directory(ProjectRecordDirectory::Knowledge,
limits)`, API Core comum com enum fechada Task/Knowledge. A seleção física não
substitui a admissão Knowledge WRITE nem aceita um caminho arbitrário.
Descartar a future não aborta o worker de aquisição: ele pode continuar esperando
e criar o lockfile e os metadados de controle normais de Core. Cancelamento e
deadline seguem os checks cooperativos do request original; Drop não comprova
quiescência nem preempção física. Esse worker não cria `knowledge`: o nascimento
ocorre somente depois do await e do recheck Auth.
O default é publicado uma vez. Para um branch nomeado, a captura precisa herdar
o N recém-publicado e ter o destino ainda ausente antes de materializar uma vez.
Falha após confirmar o default devolve `PartialDefault` com sua geração e causa. A fixture
do helper privado de finalização usa um default SDK confirmado e um seletor
inválido para comprovar a causa SDK real; seletores arbitrários não são entrada
da API pública. Os testes públicos usam grants reais de versões distintas e
corpora/pins persistidos para verificar a seleção derivada.
Não há rollback do default, replay ou retry automático. Creation failures retêm
a chave e a causa na cadeia original Foundation; não há resolver inicial novo.

O reader recebe `ProjectReadAdmission` classificada Knowledge. Memory/Task são
recusadas antes de acessar payload ou Writer. Os getters de classificação Auth
são observacionais. Os checks Data obtêm projeto e request juntos por
`project_and_request()`, com uma revalidação Auth vigente do mesmo proof.
Preservam o check do Original e a igualdade da Arc, e usam a captura emprestada
para comparar escopo/versão e derivar o seletor SDK de `version.lance_branch()`.
Essa observação não fica em cache nem substitui os checks das próximas
fronteiras. String vazia seleciona default;
string não vazia seleciona o branch codificado por Core. As APIs públicas não
recebem um seletor separado. Uma versão lógica diferente exige seu próprio grant
Auth válido. Um target nomeado ainda ausente herda o default, conforme a API
Branch; um target materializado mantém seu pin mesmo quando o default avança.
READ nunca adquire Writer. SDKs e streams são locais
à operação; retornos owned e dados de geração não conferem autoridade.

Os helpers Index normalizam somente `AuthError::Request` observando novamente
o mesmo Original: uma falha concreta conserva seu `io::Error`; Original vivo
mantém Auth Request. Outros erros Auth e causas IO existentes permanecem
intactos. Isso também vale para a captura semântica e não altera resultados de
publicação já observados, recovery keys ou incerteza de commit.

`CapturedProjectDocumentDirectory::capture` com caminho relativo vazio captura
o owner root real. O reader percorre exatamente `brand.directory()`, `runtime`,
`knowledge`, `index.lance`, com probe de um child e revalidação antes/depois.
`DirectoryPresence::Absent` de um probe válido produz `NoCorpus`, sem geração.
Se o diretório existe, somente `DatasetNotFound` no open SDK do Main sem pin
permite a mesma observação de ausência de Main publicado. Captura, identidade,
Auth e request são revalidados antes e depois desse await, inclusive em erro SDK.
Fragments preparatórios podem permanecer e não são lidos, adotados ou apagados.
Schema incorreto, manifest corrompido, `VersionNotFound`, IO/permissão e falhas de
captura/pin/Auth/request continuam erros. Ausência de Main não prova rollback nem
ausência de histórico. Uma nova publicação exige novas admissões, fontes
capturadas e writer real; o SDK Create/CAS exclusivo impede sobrescrever Main.

O schema físico fechado contém os 16 campos H abaixo e duas colunas adicionais,
`document_state` e `reference_projection`, descritas no
[contrato de referências](rust-knowledge-reference-storage.md). O DTO público
`KnowledgeDocument` conserva 16 campos. Metadata de fields/children permanece vazia:

| Campos | Tipo e nullability |
| --- | --- |
| slug, title, body, kind, summary, source_path, recipe | Utf8, obrigatório |
| description | Utf8, nullable |
| summary_truncated | Boolean, obrigatório |
| physical_digest | FixedSizeBinary(32), nullable |
| compilation_digest | FixedSizeBinary(32), obrigatório |
| recipe_parameters | Struct obrigatório: fallback_title/kind_on_absent Utf8 obrigatórios, max_summary_bytes UInt64 obrigatório |
| updated_at, stale_since | Timestamp nanosecond UTC, nullable |
| stale_after | Duration nanosecond, nullable |
| resolved_wiki_targets | List obrigatória de item Utf8 obrigatório |

Body conserva os bytes fornecidos por H; título vazio literal e targets raw
ghost/self/duplicate/empty permanecem. Digests são binários tipados. Observações
de compilação não são páginas e são recusadas neste publisher. Recipe usa os
parâmetros authored-document-v1. Tempos convertem exatamente para signed i64 ns,
inclusive antes do epoch; duração presente precisa ser positiva e ter updated_at.
Overflow/zero/range inválido falham antes do SDK, sem truncar ou ampliar tipos.

`KnowledgeIndexContentLimits` contém somente `max_identifier_bytes`, aplicado
individualmente aos identificadores validados. Read e Write reutilizam esse
contrato. Não há quota agregada de páginas, referências cruas, texto de entrada
ou texto copiado de saída. Títulos, bodies e metadata editorial não recebem
um limite de identifier por conveniência.

Wiki targets preservam valores vazios, duplicatas e self sem cobrança agregada.
`batch_rows` configura transferência SDK; os caminhos que validam o tamanho
físico do batch conservam esse check. Não é quota de corpus ou garantia de RSS.

Data conserva builders Arrow, schema físico de 18 campos, DTO público de 16,
digests, timestamps e conversões checked. Não existe `RowLimitsKnowledge` ou
quota de bytes serializados por row. Artifact/Payload encaminhados ao writer
não limitam a imagem Lance; Core protege as capturas nativas correspondentes.
Lifecycle reutiliza a gramática privada compartilhada sem o antigo agregado
global. A API pública `decode_managed_knowledge_source_path(value,
max_field_bytes)` e seus limites individuais permanecem inalterados.
Isso não entrega o futuro pipeline por arquivo ou remove a retenção atual.

Cada read captura um pin, drena scan sem sort global SDK até `None`, valida tudo,
ordena por bytes UTF8 do slug e rejeita duplicatas antes de devolver o corpus.
Estado e witness são validados antes de excluir Removed. Todos os documentos
removidos resultam em corpus publicado sem páginas; não equivalem a EOF zero.
A projeção ausente de um documento Active significa referências incompletas.
`PublishedEmpty` na leitura selecionada requer schema inteiro, geração real
e EOF zero; difere de `NoCorpus` e de um catálogo sem páginas após filtrar Removed. Uma falha não expõe owner parcial. Instalar C, decidir a política de
NoCorpus e obter J do mesmo corpus/geração por leitura própria são obrigações do
caller M1b; este produtor não instala cache nem usa C como origem de J.

Auth, original request e B1 são rechecados nas fronteiras públicas SDK/stream e
antes de expor dados. Foundation `create` combina prepare/commit em uma API sem
callback interno. Não há promessa de fence no meio de fases ocultas, confinamento
contínuo de pathname, ausência concorrente estável ou imunidade a GC. A primeira
causa SDK prevalece sobre checks posteriores.

Fixtures correspondentes: `tests/wiki_corpus.rs` usa issuer/runtime-key reais;
o teste `nonempty_full16_create_reopen_scan_roundtrip` cria payload não vazio,
reabre o SDK e compara os 16 campos documentais. A evolução física para 18 campos
é verificada também em `tests/knowledge_index_page.rs`, com referências e Removed.
Encoder ou schema isolado não qualifica esse roundtrip. Comandos e limites da qualificação ficam na
[referência de storage](../development/rust-storage-contracts.md#native-knowledge-first-publication).

Esta página orienta os produtores e consumidores da migração de Knowledge/Wiki
em `graphit-code`. **Os packets abaixo são requisitos em refinamento, não APIs
Rust disponíveis.** `tsk-84d5` mantém contratos, fixtures e lacunas; `tsk-a59d`
refina a publicação compartilhada de Records. O refinamento Lance readonly em
`tsk-0a36` ainda não entrega corpus documental completo, busca ou writer.

[Knowledge/Wiki Rust](rust-knowledge-wiki.md) descreve o kernel de referências
implementado e as outras folhas. Esse kernel não concede autoridade nem fornece
um snapshot de storage. A coordenação da sessão `ses-c98a` pertence ao principal.

## Retirada explícita do branch Knowledge nomeado

Data expõe `retire_native_knowledge_index(original, admission, limits)` em
Linux. Knowledge WRITE, com `MutateModule(Knowledge)`, autoriza somente o
branch nomeado da versão originalmente admitida; default é recusado. O wrapper
preserva request, captura e rechecks e usa `version.lance_branch()` da
observação conjunta admitida de projeto e request. Não cria corpus, não materializa
um target herdado nem altera o inventário. A confirmação de efeito conserva a
geração observada e a causa posterior, quando houver.

O [contrato de retirada](rust-knowledge-retirement.md) reúne os wrappers Data
já qualificados e as fronteiras com Core/App/Runtime. Metadata de branch e
cleanup físico são efeitos distintos. O wrapper consome o SDK real, sem
force-delete, genericNotFound, replay como confirmação ou promessa de CAS e
proteção contra GC. Retirada do índice não concede mutação de Task, Memory ou AST.

O consumer de inventário acompanha uma retirada explícita confirmada por seu
produtor real. Core `remove_version` altera metadata e rejeita checkout
registrado selecionando a versão; não apaga datasets. Se falhar depois do efeito
Data, o consumer conserva o resultado externo e a causa de inventário. Não há
transação ou rollback entre esses stores. Parking, shutdown e remoção de cache
não são intenção de retirada. A confirmação do corpus Knowledge não afirma que
outros módulos foram apagados; composição usa somente efeitos reais autorizados,
sem ledger ou metaprova universal como novo gate N5. Retain/publicação continuam
independentes da confirmação de retirada de um corpus.

## Responsabilidades e jornadas

| Jornada | Knowledge | Produtores necessários |
| --- | --- | --- |
| Indexar documentos | Validar, compilar e calcular o delta de um conjunto completo | Core/FS: seleção e captura; Auth: admissão; Data: publicação recuperável |
| Reindexar sem mudanças | Distinguir ausência de delta, reparo de projeção e manutenção | Data/Records: gerações, histórico e manutenção |
| Ler página, fontes, catálogo e referências | Selecionar projeções e interpretar resultados | Data: snapshot completo; Auth/Hub: escopo e acesso real |
| Buscar em uma ou várias wikis | Definir plano de busca e identidade das fontes | Data: FTS/dense/hybrid; AI: política compartilhada pertinente |
| Gerar embeddings | Preparar o input efetivo e descartar resultados atrasados | AI: modelo/inferência; Data: atualização condicional e lifecycle |
| Exportar ou consumir pacote nativo atual | Selecionar corpus/schema e validar sua semântica | Data/Core/FS: versão congelada e artifact; Auth/Hub: publicação e mount |
| Reset, remoção e recuperação | Solicitar efeitos explícitos e interpretar seus outcomes | Data/Records/Auth: fences, recuperação, retenção e autoridade |

Knowledge não implementa engines, grants, store provisório ou handles que imitem
produtores ausentes. Os tipos compartilhados de documento compilado, referência
tipada e identidade de modelo precisam de acordo entre os donos antes das folhas
consumidoras. Manifests, lockfiles e interfaces compartilhadas ficam na coordenação.

## Snapshot completo e paginação: P-S

O consumidor precisa do request original, admissão de leitura real, namespace e
geração fixada. As projeções incluem páginas, corpo, fontes/trechos, wiki-links,
referências tipadas com seu head, histórico, metadata e pending embeddings.
Retornar valores não transfere grants nem permite reconstruir um request com o
mesmo deadline para contornar cancelamento ou revogação.

O produtor deve vincular o cursor opaco à geração, namespace, grant, request,
query, filtros, projeção, schema, ordenação e posição. Reutilização incompatível
deve falhar. A página seguinte não muda silenciosamente para uma geração nova.
Lifetimes e retenção devem proteger snapshots/cursors ativos conforme o contrato
real, sem prometer que uma versão fixada torna o grant irrevogável.

Cada página precisa declarar completude e continuação. Knowledge só calcula
deleções depois de coletar o conjunto completo dentro dos limites. Uma consulta
com teto de rows não prova que todas as páginas foram lidas. Exceder quota, perder
acesso ou cancelar a seleção não transforma documentos não observados em removidos.

Completude pertence a cada projeção: páginas completas não comprovam referências
ou histórico completos. Os caps agregam páginas e fontes; entregar várias páginas
individualmente abaixo do teto não pode ultrapassar o orçamento da operação.

Estes estados precisam ser distinguíveis: corpus ausente, corpus publicado vazio,
busca sem match, página ausente, corrupção, projeção incompleta, mudança de fonte,
denial, cancelamento, deadline e limite excedido. Leitura permanece observacional:
não cria tabelas, repara projeções ou faz manutenção.

## Publicação lógica recuperável: P-D

O pedido ao produtor Data/Records reutiliza o contrato de `tsk-a59d`: admissão
real, request original, geração esperada, identidade da operação ligada ao
payload/ator/escopo e changeset tipado. Para Knowledge, ele abrange documentos
adicionados/alterados/removidos, vetores preservados ou invalidados, substituições
de wiki-links, gerações completas de referências tipadas, histórico e metadata.

Data precisa fornecer um ponto de publicação lógica recuperável. Isso pode usar
um intent/anchor durável; não pressupõe uma transação multitable do engine. O
head observável não pode perder histórico/referências ou combinar gerações
incompatíveis. Projeções pendentes precisam de uma verdade durável recuperável.

Os outcomes devem distinguir commit confirmado, conflito/rejeição sem efeito e
resultado incerto com identidade recuperável. Uma resposta perdida após commit
não equivale a rollback. Retry com a mesma operação/payload não duplica histórico;
reutilização do ID com outro payload/ator/escopo deve ser recusada. Recovery
reobserva head e receipt antes de reaplicar.

O handshake precisa distinguir geração de corpus/dataset, revisão do head de
página e marker de relações. Data define o alcance de cada precondition e sua
unidade lógica. A arbitragem multi-record/catalog/session de Records continua no
produtor compartilhado, sem catálogo paralelo de claims/sessions em Knowledge.
Os nomes finais do receipt devem convergir com `Applied/Rejected/Uncertain` de
Records. Falha antes da publicação significa zero alteração lógica publicada;
um intent físico já pode ter sido escrito.

Request/grant valem na tentativa original até seu prazo. Depois de restart, o
chamador precisa de nova admissão legítima para reobservar a operação persistida;
isso não restaura o grant nem renova o efeito anterior. Uma nova tentativa de
escrita exige admissão explícita e dedupe pela mesma operação/payload/ator/escopo.
O produtor precisa fechar a política de recovery antes do consumer.

No-op compara o conjunto completo e o valor compilado efetivo, incluindo metadata,
política e referências. Hash de fonte sozinho não comprova igualdade. Uma projeção
incompleta exige reparo explícito; manutenção pode executar mesmo sem delta, com
resultado separado do histórico de mudanças documentais.

O patch de referências conserva três intenções: omissão preserva autoria,
substituição troca o conjunto e `[]` publica um marker vazio. Wiki-links são uma
projeção separada. Dangling targets conservam identidade sem conceder acesso.
Poda só usa o marker correspondente ao head confirmado, preservando esse marker
e gerações futuras ainda não confirmadas. Retenção respeita leitores/artifacts
ativos segundo o contrato do produtor.

## Embeddings e lifecycle: P-E

O fingerprint precisa cobrir o **texto efetivamente inferido**, identidade e
revisão do modelo, dimensão e política pertinente. Alterar título/resumo sem
alterar o hash de fonte pode invalidar o vetor. O tipo e cálculo desse fingerprint
ainda precisam de acordo entre AI, Data e Knowledge.

A cycle captura pending pages sob exclusão de lifecycle, libera handles antes da
inferência e readmite a escrita depois. O produtor reobserva a página e só publica
o vetor se geração/fingerprint/modelo/dimensão esperados continuam válidos. Reset,
deleção, alteração ou vetor já publicado podem tornar o resultado atrasado.
Inference não impede reset; acesso ativo ao storage continua protegido.

A condição inclui também a incarnation/fence real de reset fornecida por Data.
Reset seguido de recriação com o mesmo slug, conteúdo e modelo não autoriza um
vetor capturado na incarnation anterior. Fingerprint de conteúdo não resolve ABA.

Vetores precisam de shape exata e valores finitos. Não cortar ou preencher uma
dimensão incompatível. Modelo/dimensão desconhecido preserva o corpus existente
e não cria um corpus ausente. Páginas sem vetor continuam válidas; disponibilidade
semântica é um estado separado. Reset conhecido precisa da publicação recuperável
de P-D, e não de drops sequenciais.

## Busca, identidade, fontes e trechos: P-Q

As capacidades exigidas são lexical sobre corpo e metadata separados, semantic e
hybrid, com filtros antes de ranking. Metadata de busca não repete o corpo. A
política de candidatos/fusão/ties e os tipos finais dependem de Data/AI; defaults exigem contrato explícito.

`top_k` é positivo explícito. Todos os matches usam paginação/completude dentro de
caps, sem um zero que se torne default do SDK. Orçamento de candidatos é separado
do orçamento de output; truncar candidatos não prova recall integral. Contagens
exatas, estimativas e busca vetorial aproximada precisam ser identificadas.

Identidade inclui namespace/source/context, slug e geração. Duas fontes podem
conter o mesmo slug e devem continuar distinguíveis. Falhar uma fonte não permite
silenciá-la e declarar a consulta global completa. O principal aceitou resultado
integral por padrão. Resultado parcial exige escolha explícita do caller antes
da execução e permite somente erro operacional de origem admitido, com recibo
bounded e cobertura/completude por origem e projeção. Denial, contexto original
cancelado/expirado ou grant inválido nunca se convertem em sucesso parcial. Fonte
negada não libera dados ou contagens. Essa política é um aceite de design; a API
e os produtores Data/Auth ainda não foram entregues.

Trechos devem ter ranges UTF-8 válidos no corpo/fonte capturada e projeções
limitadas. Truncamento editorial indicado é diferente de hardquota. O refinamento
de parser `tsk-b210` separa ranges Markdown em bytes originais de marcadores
diagnósticos YAML. Tags são metadata authored, não reconstruídas na leitura.

### Consumidor usado e delta delimitado

- Hits devem conservar a identidade da fonte e do contexto, e falhas ao abrir uma fonte precisam aparecer na cobertura do resultado.
- Páginas de duas fontes com o mesmo slug continuam distintas durante recuperação e síntese; carregamento anterior em uma fonte não satisfaz a outra.
- Fusão/ranking em uma única fonte não demonstra completude ou tratamento de falhas de uma consulta multisource.

### Chamadas e recibo necessários — proposta sem API Rust entregue

| Etapa do consumidor | Contrato a consumir | Evidência exigida antes de avançar |
|---|---|---|
| Selecionar fontes previstas | Identidades de origem registradas, com admissão Auth/Hub real | O conjunto esperado é explícito e limitado; registro não concede leitura. |
| Abrir leitura de cada fonte | Data snapshot/projeção autorizada com original `SnapshotContext` | Origem, geração/schema capturados e estado sem corpus/publicado vazio distinguíveis; leitura não cria ou repara. |
| Consultar e paginar | Query Data real, filtro/ordem/projeção e orçamento positivos | Custos reais e completude da projeção; cursor, se oferecido, vinculado a origem, geração, query, filtro, ordem, projeção, grant e request. |
| Ler corpos/trechos/refs solicitados | Projeções da identidade origem + slug + geração | Corpo, range e referências correspondem ao valor capturado; repetir A/Design não suprime B/Design. |
| Formar resposta Knowledge | Resultados e recibo bounded de cobertura | Não converter falha em vazio nem apresentar cobertura global completa com fonte falha, não consultada ou não exaurida. |

Essas etapas descrevem interações requeridas, não novas funções, traits ou DTOs.
O mesmo contexto original atravessa admissão, abertura, consulta, páginas,
corpos e referências. Criar contexto novo com deadline igual não preserva essa
autoridade. Cancelamento entre etapas interrompe chamadas subsequentes; o
produtor define o ponto de admissão/cancelamento e o encerramento de IO já ativo.

O recibo precisa separar cobertura das fontes admitidas/consultadas/exauridas,
completude por projeção, candidatos/trabalho efetivamente consumidos,
resultados/bytes/cursors consumidos e truncamentos. Coverage completo não afirma
recall integral de busca aproximada. Candidatos limitados não equivalem a output
limitado. Recibo, diagnósticos e metadados também têm caps agregados; uma fonte
negada não libera seus dados, contagens ou diagnóstico sensível. A forma concreta
e a exposição permitida pertencem ao contrato Data/Auth aceito.

Os limites agregam todas as fontes e páginas da mesma consulta. Um limite por
fonte ou por página não substitui o total. Se Data não conseguir fornecer/admitir
o custo de candidatos, o consumidor não pode usar o número de hits retornados
como prova desse orçamento. Top-k, candidatos, resultados, páginas solicitadas,
bytes projetados, cursores e IO/trabalho precisam das unidades e regras de
cobrança do produtor.

### Fixtures completas do packet Q1–Q8

As fixtures devem declarar entradas, ação, invariantes e resultados esperados de cada fronteira. Números de fixture não são defaults públicos; um exemplo documental não é uma suíte executada nem uma API implementada.

| Caso | Preparação e ação | Resultado exigido |
|---|---|---|
| Q1 identidade | A/Design=`body-A`, B/Design=`body-B`; buscar, solicitar ambos e repetir A/Design | Duas identidades/atribuições; ambos os corpos projetados; A repetida não duplica A nem elimina B. Gerações são independentes por fonte. |
| Q2 ausência/vazio | B publicada vazia, B ausente, B corrompida e página ausente em corpus publicado não vazio, em execuções separadas | Vazio completo é sucesso de consulta exaurida, distinto de ausência e erro; zero criação/reparo. Não revela estado sem autorização. |
| Q3 fonte falha | A responde, B tem erro operacional permitido; executar sem escolha parcial, com opt-in anterior e com opt-in somente após falha. Repetir no modo parcial com denial, contexto original cancelado/expirado e grant inválido | Integral é padrão e falha; opt-in anterior pode entregar somente parcial bounded com cobertura/completude explícitas. Opt-in tardio e falhas de autoridade/contexto não viram sucesso parcial. Nenhum dado/count da fonte negada. |
| Q4 orçamento agregado | Cap ilustrativo de 6 candidatos; páginas reportam 3 e 4 candidatos, cada uma dentro do cap local | O total 7 excede 6. Não declarar execução dentro do orçamento nem usar hits como custo; admissão/reserva prévia e término seguem produtor real. |
| Q5 contexto/cancel | Cancelar após a primeira página; tentar continuar com novo contexto de deadline igual | Não iniciar a página/fonte seguinte após cancelamento observado nem trocar lineage. Autoridade deve ser testada com issuer real na integração. |
| Q6 cursor | Trocar origem, geração, request, grant do issuer real, schema, projeção, query, filtro ou ordem do cursor, um campo por execução | Cada combinação incompatível é recusada; não abrir latest ou readmitir com contexto novo para salvar o cursor. |
| Q7 trecho UTF-8 | Corpo `ação 🍀`, offsets em bytes [1,5) e [1,2); cap editorial ilustrativo 3 bytes | [1,5) é `çã`; [1,2) corta `ç` e recusa. Truncamento editorial, se aceito, pode devolver `ç` com indicação; hardquota permanece erro. Offsets pertencem ao mesmo corpo/geração, sem misturar fonte Markdown e corpo transformado. |
| Q8 projeções/recibo | Query de páginas completa e refs associadas incompletas; esgotar também o cap de metadados do recibo | Completude distinta por projeção, sem sucesso integral falso; receipt/cobertura não crescem fora dos caps. Aproximação e truncamentos permanecem explícitos. |

A política integral por padrão/parcial opt-in acima foi aceita pelo principal.
Para abrir a folha Source, Data/Knowledge/AI ainda precisam aceitar **o contrato
concreto de resultado multisource**: campos/tipos e custos do recibo de
query/projeção/cursor, classificação dos erros operacionais permitidos,
admissão/original-context e scores/ties.
Além desse aceite, os produtores reais de query, documento canônico e grant
precisam estar entregues. `tsk-84d5` registra esse prerequisite; estes exemplos
não tornam uma folha pronta nem duplicam Data, Auth, Core ou o caller `tsk-9840`.

## Pacote nativo e efeitos de lifecycle: P-X

Exportação prende a geração e o snapshot de artifact durante validação e stream.
Validar um diretório e depois copiar seu conteúdo mutável não fornece essa prova.
Core/FS owns paths, alias/symlink, TOCTOU, formato de artifact e quotas; Data owns
leitura/publicação do corpus; Knowledge owns sua validação semântica.

Importação consome somente o formato Rust nativo atual aprovado, validado por
completo antes da publicação. Não há conversor de formatos antigos ou fallback permissivo.
Os pacotes carregam corpus e metadata de relações, sem os corpos Task/Memory
referenciados; o destino resolve acesso com seus próprios grants. Schema e
capabilities finais ainda estão pendentes.

Mount Hub readonly consulta o contexto registrado e autorizado onde reside, sem
download, recompilação, criação de tabelas ou manutenção. Registration não concede
grant. Reset/remove/reconcile/compact precisam de exclusão, fences, retenção,
restart e outcomes pós-commit reais; Knowledge não fornece mutex local como
substituto de lifecycle entre processos.

## Limites e provas necessárias

Os packets exigem caps positivos explícitos para rows, bytes decodificados,
keys/cursors, projeções, candidatos, batches/vetores, IO e duração conforme budgets
dos produtores. Valores numéricos e APIs finais ainda não foram aceitos. Nenhuma
folha pode ganhar readiness apenas por receber enums de erro ou uma trait mock.

| Fixture | Comportamento protegido |
| --- | --- |
| G1 | No-op exato, preservação de vetores/histórico; reparo incompleto e maintenance separados |
| G2 | Delta de conjunto completo; alteração de metadata/policy; parcial nunca deleta ausentes |
| G3 | Inferência fora do lifecycle, reset/reopen e recriação idêntica, vetor atrasado, cancelamento ao adquirir acesso |
| G4 | Unknown width preserva/não cria; mismatch/NaN/Inf não recebem padding |
| G5 | Marker vazio, future generation, old replay, poda pelo head e identidade entre namespaces |
| G6 | Faults/intent antes/depois de publicação, lost ack/restart, operação com payload/ator/escopo diferente, retry sem duplicação |
| G7 | Cursor/request/schema/geração errados, revogação, readonly, caps globais, export durante GC, incompletude por projeção |

As fixtures precisam de substrate/grants reais e cenários de reopen/restart para integridade. Execute apenas suítes afetadas; uma folha não exige repetir toda a integração.

## Evidência atual e próximos contratos

As regras mantidas estão em `Wiki_Module_Specification` e
`Persisted_record_relationships`. Os testes de unknown width em `tsk-fa72` não
comprovam reset atômico. A correção `tsk-ed14` exige poda contra o head confirmado;
`tsk-dece` preserva maintenance no fast path e evita geração desnecessária.

Faltam produtores aceitos de P-S/P-D/P-X, tipos compartilhados e seleção capturada
Core completa. O getter de mtime em `tsk-8fcc` não entrega seleção documental.
Depois de fechar essas interfaces, criar folhas independentes com APIs, erros,
caps, dependências concluídas, fixtures e documentação; readback e readiness
precedem código. O principal recebe o handshake e devolve IDs dos produtores.

## Completude da geração documental: J1/G2 e lifecycle

Este refinamento documenta o limite entre indexar documentos e publicar o
corpus. É um contrato futuro dos consumidores Rust, sem implementação de
captura, snapshot, writer ou lifecycle adicional em Knowledge.

A seleção autorizada e a compilação de todas as fontes selecionadas precisam
estar completas antes de calcular deleções. Um documento publicado que não
pôde ser lido continua distinto de um documento cuja remoção foi observada em
uma seleção completa. Falha de leitura/captura, parse, metadata tipada, quota,
overflow, revogação, cancelamento ou prazo não transforma fonte selecionada em
ausente. A policy de resultado parcial da busca P-Q não autoriza publicar
um conjunto desejado parcial nem deletar páginas não observadas.

A fixture de completude precisa incluir um documento novo após o primeiro índice sem alteração das fontes anteriores e uma fonte já publicada cuja leitura falha. Falha de leitura não pode virar ausência no conjunto desejado nem exclusão lógica. Use uma falha causal de leitura, sem depender de chmod que se comporte de forma diferente conforme o usuário do teste.

### Owners e fluxo

Core/FS fornece seleção/captura autorizada, integridade da leitura e evidência
real de exaustão/completude; Auth fornece admissão e revogação. Knowledge
compila cada FILE sob o request original e só prepara o desejado integral após
admitir todas as fontes. Data/Records fornece o snapshot esperado, publicação
P-D, histórico, relações, vetores, recovery e fence contra reset/GC.

Um produto compilado por FILE não comprova completude do conjunto nem concede
admissão de publicação. Captura ou metadata bem-sucedida de A não autoriza
omitir B. Nenhum bool `complete=true`, DTO de grant, digest ou contexto com
deadline igual substitui a prova do produtor real. Se a API não distingue
exaustão de interrupção, a folha consumidora permanece bloqueada por essa seam.

Os caps positivos agregam documentos, raw/decoded/projection bytes,
nodes/work/cursors, diagnósticos e proveniência. Cada documento caber
isoladamente não permite exceder o orçamento da operação inteira. Cancelamento,
prazo e erros de autoridade continuam terminais; não criar um request novo para
salvar a tentativa. Quantidades e APIs finais vêm dos produtores, sem defaults
numéricos definidos por este refinamento.

Falha antes da publicação mantém a geração lógica publicada, páginas,
histórico, refs e vetores prévios. Isso não afirma zero writes no FS: artifacts
Core ou intent Data preparatórios podem existir. Depois do commit, lost ack ou
cancelamento seguem o receipt/outcome real de P-D; não prometer rollback do
corpus confirmado. Reset/remove/recreate precisa da incarnation/fence Data,
mesmo quando slug e corpo são idênticos.

### Fixtures adicionais por propósito

Preparar corpus publicado g0 com A/B, histórico H0, refs R0 e vetores V0
válidos. Esses nomes são valores ilustrativos, sem novo DTO, grant ou enum.
As fixtures são documentais e ainda não executadas. Integração exige os
produtores reais de captura, admissão, snapshot e publicação.

| Caso | Operação e resultado exigido |
| --- | --- |
| C1 leitura indisponível | Seleção inclui A/B; A é capturada e a operação real de B falha. Propagar o erro e impedir deleção de B ou novo head/history/refgen; g0/H0/R0/V0 permanecem. |
| C2 autoridade/contexto | Repetir A→B com denial, revogação real, cancelamento original e expiry, separadamente. Sem sucesso parcial, novo contexto ou vazamento de fonte negada; usar issuer/barriers do produtor, sem sleep/corrida/fake clock Knowledge. |
| C3 compilação inválida | Captura integral de B tem UTF-8/YAML ou description/type inválidos. Erro sem produto; A isolada não vira conjunto integral que exclui B. Reusar testes qualificados por documento quando a integração estiver pronta. |
| C4 agregado/overflow | Fontes cabem individualmente e excedem um cap real agregado por uma unidade. Falha sem deleted/history/ref replacement; work admitido não é reembolsado. Rows retornadas não medem trabalho ou completude. |
| C5 remoção explícita | Selecionador autorizado exaure conjunto real somente A e comprova B fora da seleção por policy/ignore aceito. Então deleção de B usa P-D, published pin esperado e admissão atual; erro de leitura não equivale a essa observação. |
| C6 vazio | Conjunto vazio autorizado/exaurido admite plano integral; vazio por first-read failure, cap, cursor pendente ou corpus ausente não admite deleções. Preparar observações separadas, sem fallback vazio. |
| C7 lifecycle | Fixar g0; reset/remove/recreate g1 antes do commit. Fence/incarnation Data recusa o plano antigo, inclusive recriação idêntica. Lost ack póscommit segue uncertain/reobserve de P-D. |
| C8 hash e símbolos | Rawhash atual não prova autolinks/symbols ou avaliação atual completa. Manter fatos publicados e avaliação autorizada atual separados, sem fabricar marcador stale e sem alterar a recipe pura de compilação authored. |

C1/C5/C6 cobrem J1/G2 e o recorte Source lifecycle da migração. C2/C4 cobrem
admissão/contexto/caps; C7 cobre G3/G6/lifecycle; C8 preserva no-op e staleness
observacional. A adição de documento protegida pelo teste antigo permanece uma
fixture distinta de read failure e remoção.

### Hash, staleness e readiness

Conservar a separação do plano documental entre PublishedView e
CurrentEvaluation vinculada ao published pin, observações atuais completas e
autorizadas, policy e evaluated_at original explícitos. Igualdade do rawhash
não é autoridade ou freshness física. Este recorte não acrescenta um input
AST fictício à recipe pura de compilação já aceita no plano de documentos.

O GET atual de `tsk-0a36` (OPEN r14), `tsk-86ec` (OPEN r2) e `tsk-a59d` (OPEN
r49) não entrega os producers completos P-S/P-D/lifecycle. Esses records já
estão ligados no frontmatter; seus probes/refinamentos e o compilador por FILE
não substituem o handshake. Não criar uma folha Source pronta até receber
interfaces reais, IDs, caps e semântica dos owners. Este apêndice não executa
código, publica corpus, altera gates ou conclui a migração.

## Leitura Knowledge de heads para Now PROJECT

A Source `tsk-a542` acrescenta ao mesmo módulo Data
`read_native_knowledge_heads(original, admission, limits)`. Sua qualificação
Main em Linux passou 8 testes de integração, 8 de biblioteca, 20 doctests,
Clippy estrito em todos os targets Data, formatter e diff-check. A API não
entrega o compositor Now, seu transporte ou a seção Memory/Task/Session.

`KnowledgeHeadRead` distingue `NoCorpus` de
`Published { generation: KnowledgeGeneration, heads: Vec<KnowledgeHead> }`.
Cada head contém somente `slug`, `title` e `updated_at: Option<SystemTime>`.
A ausência da data e o título vazio permanecem literais, sem autor, fallback
ou data inferida. Datas usam os mesmos nanosegundos UTC assinados de FULL16,
incluindo valores anteriores ao epoch.

O reader valida o schema físico fechado de 18 campos ao abrir e projeta no
scanner SDK `slug/title/updated_at/source_path/compilation_digest/resolved_wiki_targets/
document_state/reference_projection`. Um pin da versão autenticada permanece
até EOF. Estado, origem, referências e witness são validados antes de excluir
Removed; body e recipe não são projetados. A leitura de heads não prova a
semântica dos payloads não projetados.
`KnowledgeHeadReadLimits` recebe somente `max_slug_bytes`, limites nativos
de captura e `batch_rows`, todos positivos. O slug conserva seu limite
individual. Não há contador agregado de heads, texto ou referências nem
conversão do antigo limite textual em limite de identifier.
Heads reutiliza o validator de referências com limite individual ausente;
full read, query e Write fornecem o limite Content real ao mesmo validator.
A ausência não vira default máximo. Namespace, estado, witness, nulabilidade,
unicidade e EOF continuam obrigatórios.

A admissão precisa ser Knowledge e compartilhar a Arc original; Auth,
request e captura são rechecados nos boundaries de leitura e entrega.
A seleção vem exclusivamente de `version.lance_branch()` da captura obtida
pela observação conjunta admitida de projeto e request.
`NoCorpus` segue a observação de Main ausente descrita acima: probe nativo
Absent ou `DatasetNotFound` no open Main sem pin após revalidar identidade e
autoridade. Schema inválido, corrupção, demais erros SDK, IO, revogação, deadline
e cancelamento preservam erros, sem nascimento/repair ou resultado parcial. Heads são retornados
inteiros, ordenados por slug UTF-8, e duplicatas globais inclusive entre
batches recusam a entrega.

Now Runtime é consumidor futuro: deve ordenar por data decrescente, datas
ausentes por último e slug como desempate, depois limitar a 20 com total
pré-limite e `has_more`. Generation é observação interna, não autoridade:
`corpus_uri` e branch físico não pertencem ao payload Now/local-remoto;
links usam identidade lógica de projeto/versão/slug. O reader documental
`read_native_knowledge_index` continua servindo os consumidores que precisam
do corpus completo. TaskStore ou inventário SQL não são necessários para
esta leitura nativa independente. A Source não qualifica lease/atividade,
Now CLI/HTTP/Broker, USER, Win/Mac ou coleta FULL16 como otimização.

O armazenamento preserva schema físico FULL18, leitura até EOF, witnesses, namespace/estado, CAS, causas reais e limites individuais de identificadores, sem contabilidade agregada do corpus.
