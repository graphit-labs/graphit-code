---
type: specification
title: Consulta de referências Knowledge Native importada
description: Páginas readonly da pin importada admitida, com namespace persistido, cobertura explícita e cursor da seleção observada.
references:
  - target: {type: task, id: tsk-3514}
    relation: implements
  - target: {type: task, id: tsk-8c07}
    relation: uses
  - target: {type: task, id: tsk-9d61}
    relation: uses
  - target: {type: task, id: tsk-58db}
    relation: uses
  - target: {type: task, id: tsk-c8e9}
    relation: uses
  - target: {type: task, id: tsk-41d1}
    relation: derived_from
  - target: {type: task, id: tsk-ef75}
    relation: supports
  - target: {type: task, id: tsk-185c}
    relation: refined_by
  - target: {type: task, id: tsk-fa7b}
    relation: corrected_by
---

# Consulta de referências Knowledge Native importada

Para integradores Knowledge/Data e consumidores de referências readonly.
`tsk-3514` tem Source implementada em
[imported_reference_query.rs](../../crates/graphit-knowledge/src/imported_reference_query.rs)
e no método do [reader importado](../../crates/graphit-knowledge/src/imported_knowledge.rs).
**Query qualificada no Linux por oito funções SDK distintas e um compile-fail
separado**, com check/strict/fmt/diff aprovados. Os produtores concluídos
`tsk-8c07` e `tsk-9d61` mantêm seus escopos; esta prova vem das execuções próprias
da query, descritas abaixo, não de inspeção documental.

## Reader admitido e sequência

Abra a seleção exata conforme a [leitura importada](rust_imported_knowledge.md),
com ImportedKnowledgeReadAdmission Web real, provider/política dedicada habilitada
no servidor e captura Installation. CapturedImportedKnowledge retém grant
emprestado, Registry/input, instalação selecionada, capturas e ModuleSnapshot
privado. O Original vem da admissão; não é criado um novo contexto ou prazo.
Nenhum getter público expõe snapshot/store/grant nem converte a seleção para
PROJECT, USER ou NativeCaller.

RegistryAbsent pertence à abertura de uma seleção exata ausente. Depois de obter
Published(reader), references consulta a pin retida: o método não retorna
RegistryAbsent ou NoCorpus. Root/child obrigatório ausente, captura/SDK/schema
inválidos são erros causais. O reader não instala, nasce corpus, materializa,
repara, reindexa ou publica. Artifact ConcreteVersion é literal e distinto da
versão SDK; o default observado não é uma branch derivada da versão do artifact.

Cada operação revalida grant, Registry/full selected tuple, Original e capturas
antes da consulta, nos boundaries Data de await/batch/row/EOF e antes da entrega.
Uma nova seleção/provider/captura inválida recusa a operação. O mesmo reader
conserva sua pin mesmo após publicação SDK externa, sem promessa de imunidade a
GC; um novo reader observa sua própria geração. A metadata da observação e o
namespace dos registros não concedem OwnRead dos destinos.

## API e dados

O método async CapturedImportedKnowledge::references(&self,
query:ImportedKnowledgeReferencesQuery<'_>,limits:KnowledgeIndexReadLimits)
retorna Result<ImportedKnowledgeReferencesRead,ImportedKnowledgeReferenceError>.
A query é emprestada, com os campos:

| Campo | Tipo e regra |
|---|---|
| outgoing | Option<&str>, slug exato |
| incoming | Option<KnowledgeLogicalTarget>, identidade lógica literal |
| page_size | NonZeroUsize, 1..100 |
| outgoing_cursor / incoming_cursor | Option<&ImportedKnowledgeReferencesCursor>, por direção |

Pelo menos um foco é necessário; cursor requer seu foco correspondente.
KnowledgeLogicalTarget reutiliza ReferenceTargetKind/LogicalScope, id e context
emprestados do [adapter nativo](rust_knowledge_reference_query.md). A conversão
pub(crate) para Data é compartilhada; não há kernel ou parser duplicado. Contexto
presente é exclusivo de Knowledge, literal e não vazio. Um incoming target não
precisa existir e não solicita grant, store lookup ou body do destino.

`ImportedKnowledgeReferencesRead` contém
`focused_source: Option<KnowledgeReferenceSource>`,
outgoing/incoming como Option<Box<ImportedKnowledgeReferencePage>>,
coverage:KnowledgeReferencesCoverage e
warnings:Vec<KnowledgeReferencesWarning>. Cada página conserva results,
total, complete e next_cursor.

O wrapper move focused_source integralmente do resultado Data, sem inferir
namespace pelo alias ou mount da instalação. A fonte owned pertence à
mesma pin observada pelas páginas, com slug/title/source_path/digest
literais e namespace/witness opcionais. AliasMount não substitui o
namespace persistido e nenhuma dessas observações concede OwnRead.

Outgoing Active com projeção válida vazia conserva Some com namespace
e witness Some. Projeção ausente conserva Some com ambos None e os
warnings/completude atuais. Incoming-only retorna None. UnknownSource
permanece erro; o método references de um reader Published não passa
a produzir NoCorpus ou RegistryAbsent.

A transferência ocorre somente no resultado Data bem-sucedido após
dois EOFs e os guards Original/Auth/Source existentes. Captura, Registry,
geração e cursors permanecem vinculados às suas observações atuais.
Erro tardio conserva a causa real, sem fonte parcial ou fallback vazio.

Cada próxima continuação mantém o cursor importado existente. Edge/payload/source/
observation/coverage/warnings são os tipos Data existentes, sem DTO paralelo ou
cópia do corpus; somente os próximos cursors recebem o wrapper importado.
reader.observation().generation acompanha as duas páginas como geração real
observada. Digest de compilação e witness identificam a revisão documental/de
referências; não são um counter inventado, grant ou geração SDK.

Namespace, scope/id/context, relação, field, ordinais e proveniência permanecem
como armazenados. Alias do mount e artifact producer não substituem stored
namespace. Authored preserva ordem, duplicatas e strings literais. WikiLink é
payload distinto: preserva duplicatas/self-links e ordinal físico; o target vazio
é contado no input/witness, mas não emite edge. Não é reconstruído a partir de
body nem normalizado pela saída de ReferenceGraph.

KnowledgeHead conhecido só usa metadata da origem ativa/namespace persistido e
mesma pin; targets estrangeiros ou sem correspondência permanecem Unresolved.
Um título observado não concede leitura. Active sem projeção gera
MissingCurrentProjection e cobertura incompleta, sem inferir namespace/witness.
Removed precisa ter projeção válida/refs e wiki vazios antes de ser excluído.
Outgoing ausente retorna UnknownSource após EOF; incoming sem matches é uma
página publicada vazia. Complete descreve projeção, pode coexistir com
next_cursor e não significa completude global entre fontes/módulos.

## Pin, scans e limites

A implementação delega integralmente à API guarded Data de
[knowledge_reference_query.rs](../../crates/graphit-data/src/knowledge_reference_query.rs).
Ela verifica URI real/versão SDK/schema18 da snapshot e realiza dois passes
independentes completos na mesma pin. A projeção tem exatamente oito campos:
slug, title, updated_at, source_path, compilation_digest, resolved_wiki_targets,
document_state e reference_projection. Bodies não são carregados por esse scan. Rows Removed e listas cruas Wiki,
incluindo vazios, duplicatas e self, são validadas antes da emissão de edges,
sem ledger agregado.

O primeiro pass valida dados e witnesses e conserva metadata ativa e
contagens funcionais. O segundo repete validação até EOF e retém apenas
as páginas pedidas, no máximo 100 por direção. Não conserva todo histórico
ou edges nem depende da ordem do scanner. Erro tardio descarta a resposta
inteira; totais e offsets mantêm aritmética checked.

O método recebe `KnowledgeIndexReadLimits` diretamente. Identificadores
lógicos obedecem a `max_identifier_bytes`. Não há soma textual de entrada,
quota de texto copiado ou conversão de URI/branch em identifier. Framing e
limites reais de transporte pertencem ao consumidor.

Binding Data preserva pin, geração, seleção, focos, direção, offset e policy
individual de identifier; batch SDK não integra identidade de cursor.
Binding imported preserva Registry, instalação, SourceRef, identidade,
versão concreta e alias; alias não reescreve namespace persistido.
Cada chamada revalida capturas e grant reais. Não há parser novo, getter
de knobs privados Core ou grant por locator.

## Cursor da seleção importada

ImportedKnowledgeReferencesCursor tem Clone/Debug/Eq e Serde fechado
(deny_unknown_fields), com campos privados binding:[u8;32] e
cursor:KnowledgeReferencesCursor. O wrapper compara uma observação compacta da
seleção; o cursor Data conserva URI/SDK version/physical branch/inherited target,
focos/direção/offset e limites semânticos. CallerPin e Native Data são bindings
distintos; um cursor Native não vira importado por alterar o envelope.

O digest é SHA256 do domínio UTF8 `graphit.knowledge.import-reference-cursor`
terminado pelo byte NUL (0x00), seguido
pela serialização JSON compacta da tuple
[1,registry.scope(),Installation completa,artifact ID,ConcreteVersion,alias Option].
A serialização usa serde_json::to_writer streaming, sem Vec de JSON. Participa
a Installation completa, inclusive SourceRef origin/producer/type/logical e
physical IDs/version/root/directory; as restrições Native/Original sem binding
especial já foram validadas na abertura. A escrita do hash verifica o mesmo
Original cooperativamente antes/durante/depois e preserva a primeira causa I/O.
Hub Installation continua dado Serialize, sem criar Deserialize/parser público.

Alterar seleção completa/artifact/version/alias invalida o wrapper, mesmo com
URI/pin iguais. O digest é dado observacional: **não é HMAC, autenticidade,
credencial, epoch ou autoridade**. Mudança de binding/direção/foco/limite semântico
recusa continuação. Page_size pode mudar dentro 1..100. Offset>total é inválido
após EOF; offset==total permite página vazia. Um offset alterado ainda válido
não é necessariamente recusado por autenticidade; o cursor não oferece essa
promessa. Revalidação real do reader permanece necessária em todas as páginas.

## Erros e primeira causa

ImportedKnowledgeReferenceError tem Reader(ImportedKnowledgeReadError),
Query(KnowledgeReferenceQueryError), InvalidCursor, Request(io::Error) e
BindingSerialization(serde_json::Error). Error::source encaminha as causas reais.
O callback externo conserva a **primeira** ReaderError tipada e devolve apenas um
sentinel privado Data; após a consulta, devolve essa ReaderError, não uma
classificação por Display/string ou erro convertido em ausência. Sem causa no
slot, o erro Query/Data/SDK/request/witness/limite permanece causal.

Um erro Original de Data pode ocorrer antes do callback e continua Query(Data),
sem reescrever como erro Auth. No hash streaming, a primeira causa I/O original
prevalece sobre o wrapper serde e retorna Request; erro real de serialização
continua BindingSerialization. Cancelamento/deadline são cooperativos, sem
promessa de preemptar SDK, rede de UserInfo adicional ou healthy prefix.

<a id="validação-pendente-e-consumidores"></a>

## Qualificação Linux e consumidores

Oito funções SDK distintas foram qualificadas: seis casos passaram na execução
fullsdk03 (6 PASS/1 FAIL por fixture Hub alias inválida antes da query); a função
de cursor corrigida passou no rerun exato04; e o novo caso T3 de causa SDK real
passou em 05. Os reruns não repetem a suite nem acrescentam funções sobrepostas.
Product Source permaneceu inalterado de 01 até 05. Em 04 mudaram somente a fixture
de substituição SourceRef válida, tamper determinístico do digest e URI Data
oversized; em 05 foi acrescentado o novo T3 com remoção real de datafile SDK e
troca da tuple SourceRef completa durante await.

A prova cobre Web/Registry/Core/SDK reais, namespace/provenance literal, alias e
seleção/pin/cursor, páginas/caps, Removed/empty/projeção ausente e primeira causa
dos guards. A abertura/consulta recusam mudanças reais de captura/Registry/Auth
sem transformá-las em ausência. Erro SDK sem falha externa permanece Query/Data;
a primeira falha do reader continua Reader tipada. Nenhuma prova de GC está
incluída nem há promessa de estabilidade infinita de uma pin.

Check lib01 e compile05 passaram; strict05 com --lib --test imported_reference_query
--no-deps -- -D warnings passou. O compile-fail doc03 da captura privada passou
como **um caso separado**, não uma função SDK. Owned fmt7/diff10 passaram.
Os comandos focais da qualificação são:

- cargo test --offline --locked -p graphit-knowledge --test imported_reference_query -j1
- cargo clippy --offline --locked -p graphit-knowledge --lib --test imported_reference_query -j1 --no-deps -- -D warnings
- compile-fail documental da captura privada e fmt/diff/links dos arquivos próprios.

O histórico conserva compile/strict02 E0599 da fixture que supunha
RequestInterrupted. A asserção foi corrigida para AuthError::Request (unit) e
KnowledgeIndexError::Request(io::Error), sem mudar Product Source ou reescrever
causas. O fullsdk03 InvalidData de alias Hub ocorreu antes da query; a correção
da fixture válida e rerun exato permanecem auditados. Nenhuma falha é promovida
a PASS histórico, e o custo de fila/wall não é benchmark.

Esta qualificação é a API native imported-reference readonly em Linux; não
qualifica App/FullNoLoss, Browser/IAB, OwnRead USER/PROJECT dos destinos,
publisher/importer, Runtime, ModelService/RPC, outras plataformas, GC ou
performance. Root `tsk-ef75` recebe esta API para composição autorizada,
sem converter coverage de uma fonte em resultado global completo. Lifecycle da
Task é independente destes resultados; receipts e Source não são alterados
pela manutenção documental.

O armazenamento preserva schema físico FULL18, leitura até EOF, witnesses, namespace/estado, CAS, causas reais e limites individuais de identificadores, sem contabilidade agregada do corpus.
