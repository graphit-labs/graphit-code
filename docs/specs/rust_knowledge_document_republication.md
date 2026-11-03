---
type: specification
title: Republicação de documentos Knowledge existentes em Rust
description: Captured compiled inputs, autoridade dupla, imagem física completa e CAS sem retry.
references:
  - target: {type: task, id: tsk-e6b2}
    relation: implements
  - target: {type: task, id: tsk-f02f}
    relation: implements
  - target: {type: task, id: tsk-41d1}
    relation: derived_from
  - target: {type: task, id: tsk-8fe2}
    relation: supports
  - target: {type: task, id: tsk-72ce}
    relation: uses
  - target: {type: task, id: tsk-8b24}
    relation: uses
  - target: {type: task, id: tsk-9983}
    relation: uses
  - target: {type: task, id: tsk-c8e9}
    relation: uses
  - target: {type: task, id: tsk-bd00}
    relation: uses
  - target: {type: task, id: tsk-190c}
    relation: uses
  - target: {type: task, id: tsk-eb40}
    relation: uses
  - target: {type: task, id: tsk-9d61}
    relation: uses
  - target: {type: task, id: tsk-c9e2}
    relation: refined_by
  - target: {type: task, id: tsk-185c}
    relation: refined_by
---

# Republicação de documentos Knowledge existentes em Rust

Para integradores da biblioteca Knowledge que precisam substituir documentos
`Active` já publicados a partir de arquivos explicitamente capturados e
compilados. O produtor implementado atende ao escopo Native Linux.
**Qualificada em Linux por 31 funções distintas: 14 novas de republicação
(Data Native8, phases SDK3 e Knowledge Captured FILE3) e 17 testes existentes
do módulo índice/codec afetado. Strict focal e owned fmt/diff passaram.**

## Jornada e fronteiras

O consumidor lê a geração existente com `ReadKnowledge`, captura e compila os
novos arquivos com o mesmo Original, prepara `AuthoredDocumentInput` e chama
`republish_native_documents` com aquela geração e os dois admissions reais:
`ReadKnowledge` e `WriteKnowledge`. Ambos devem pertencer ao mesmo Original,
projeto, raiz física e versão selecionada. Não é possível obter autoridade de
um digest, geração, URI, cursor, página ou de um admission de escrita sozinho.

O conjunto de entrada é finito e explícito. Cada slug deve existir e estar
`Active`. A operação substitui todos os campos públicos daquele documento,
incluindo whole source, os dois digests, parâmetros de receita, timestamps,
staleness e a lista literal de wiki targets. As demais linhas físicas, inclusive
`Removed` e suas projeções, são preservadas.

Primeira publicação continua com `publish_native_documents` ou com o produtor
Data completo de referências. Inserir, remover, reativar e materializar uma
branch são efeitos de lifecycle separados; esta API não os combina. A leitura
importada permanece readonly e está documentada em
[referências importadas](rust_imported_reference_query.md).

## API pública

```rust
pub struct NativeDocumentRepublicationLimits {
    pub publication: NativeDocumentPublicationLimits,
    pub read: KnowledgeIndexReadLimits,
}

pub async fn republish_native_documents(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    expected: &KnowledgeGeneration,
    documents: &[AuthoredDocumentInput<'_, '_>],
    limits: NativeDocumentRepublicationLimits,
) -> Result<KnowledgeDocumentUpdateResult, NativeDocumentRepublicationError>;
```

`publication` reutiliza parsing, mapping, fingerprint, projeção H e escrita
do índice. `publication.references: ReferenceLimits` limita identificadores do
grafo; `publication.authored_references: DocumentAuthoredReferenceLimits` limita
saída authored 1:1 por documento e seus identificadores, e é o campo passado à
factory authored. As duas políticas são fornecidas explicitamente, sem soma
textual ou quota de corpus do grafo.
A qualificação atual de `tsk-c9e2` passou em Linux: 157 funções afetadas,
Clippy estrito, compilação das duas fixtures App e três doctests modificados.
O recibo `.build/rust-knowledge-reference-limits-cleanup/validation-current.json`
separa falhas iniciais e replays. Windows e macOS não foram executados; os
receipts históricos permanecem intactos. `read` limita a leitura da imagem
existente, os captures Native e o batch scan. Não há políticas implícitas,
renovação do Original ou parâmetros livres de seleção física.

O adapter conserva os owners de `ParsedDocument`, inclusive strings YAML
decodificadas, até o retorno do await Data. Ele reutiliza a compilação capturada,
mapping/fingerprint e H existentes. Data não depende de Records e não parseia
documentos para inferir autoridade ou referências.

O produtor Data também está disponível para consumidores com H já preparada:

```rust
pub enum KnowledgeDocumentAuthoredUpdate<'a> {
    Preserve,
    Replace(&'a [KnowledgeAuthoredReference<'a>]),
}
pub struct KnowledgeDocumentReferenceUpdate<'a> {
    pub slug: &'a str,
    pub authored: KnowledgeDocumentAuthoredUpdate<'a>,
}
pub struct KnowledgeDocumentUpdateLimits {
    pub read: KnowledgeIndexReadLimits,
    pub write: KnowledgeIndexWriteLimits,
}
```

`update_existing_native_knowledge_documents` recebe Original, read, write,
expected, `PreparedKnowledgeDocumentRows`, a lista acima e os limites. A lista
de decisões de referências deve cobrir os headers H exatamente uma vez.
Duplicatas, entradas desconhecidas e cobertura incompleta são erros.

## Referências e imagem completa

| Decisão | Projeção anterior | Resultado |
|---|---|---|
| `Preserve` | `None` | Continua `None`; a lista wiki nova permanece literal. |
| `Preserve` | `Some` | Conserva namespace e tuplas authored ordenadas; recalcula witness com o novo header/wiki. |
| `Replace([])` | Qualquer | `Some` completo vazio, com namespace do projeto nativo sem contexto. |
| `Replace(tuplas)` | Qualquer | `Some` completo literal, com ordem e duplicatas da entrada. |

No adapter, metadata ausente corresponde a `Preserve`; `references: []`
corresponde à limpeza explícita. `null`, mapping ou sequência inválida são
erros do produtor authored metadata existente. Tipos Task/Session/Memory/
Knowledge, PROJECT/USER, contexto Knowledge, relação e field mantêm seus
valores literais. `None` nunca é convertido em uma falsa resposta completa.

A imagem persistida usa o schema físico exato de 18 colunas. As 16 colunas da
DTO de leitura permanecem iguais. As colunas adicionais são estado e projeção
nullable de referências; o witness C8 vincula cabeçalho, digest de compilação,
estado, namespace, authored e wiki targets ordenados. A descrição completa do
schema e da consulta está em
[referências Native](rust_knowledge_reference_query.md).

O produtor valida todas as linhas da mesma pin até EOF: schema, nulls, tipos,
recipe, datas, digests, wiki targets, estado, namespace e witness, inclusive
linhas `Removed`. Slugs físicos duplicados são recusados. A imagem de overwrite
usa slices dos batches físicos; a DTO pública de páginas não é uma imagem de
overwrite porque omite linhas `Removed` e as projeções adicionais.

## Concorrência, no-op e efeitos confirmados

O writer cooperativo existente é adquirido fora do executor async. Sob esse
writer, o produtor recaptura o diretório e a pin selecionada pelos admissions,
e compara exatamente URI, versão SDK, branch física e target inherited contra
`expected`. A ausência do corpus retorna `NoCorpus`; não nasce um corpus novo.
Falhas de capture, Auth ou SDK mantêm sua causa e não viram ausência.

A comparação de no-op usa os valores lógicos Arrow de todas as colunas da linha
substituída. Mesmo uma entrada vazia ou idêntica valida autoridade, expected e
a imagem até EOF. Um no-op retorna `Unchanged { generation }`, sem preparar
ou publicar transação nem materializar uma branch.

Uma mudança em target inherited retorna `InheritedTarget` antes de preparar,
sem escrever na branch pai. Uma branch física default ou nomeada existente
usa o `BranchSnapshot.prepare_overwrite` e o CAS de versão SDK com zero retry.
O writer permanece vivo até o resultado concreto. Pins anteriores conservam
sua observação anterior; o produtor não retira ou recria o corpus.

| Resultado | Interpretação e ação do consumidor |
|---|---|
| `Unchanged { generation }` | A imagem é idêntica sob expected atual. |
| `Updated { generation, recovery_key }` | O SDK confirmou uma nova geração. Usar essa observação em leituras seguintes. |
| `GenerationChanged { observed }` | Expected diverge. Reler e decidir se a intenção ainda deve ser aplicada. Sem retry automático. |
| `Commit(DeltaCommitFailure)` | Conservar a chave real e a causa SDK para investigação/resolução autorizada. Não inferir rollback nem repetir o CAS. |
| `Published { generation, recovery_key, cause }` | Commit confirmado, seguido de falha de guard/Original na entrega. A nova geração/key são reais. Não reportar a imagem anterior como atual. |
| `InheritedTarget` | Executar a [materialização explícita autorizada](rust_knowledge_branch_materialization.md) antes de tentar uma mudança, usando a geração física realmente retornada. |
| `UnknownSource` / `InvalidInput` | Corrigir slug/estado ou cobertura da entrada; nada é inserido ou reativado. |

`NativeDocumentRepublicationError` mantém as causas Mapping, Authored,
Projection, Data e Allocation. `Error::source` preserva a cadeia causal.
Preparação SDK pode deixar fragments não publicados; cancelamento depois de
preparar não equivale a ausência de efeito físico. Apenas o sucesso real do
commit autoriza `Updated` ou o erro confirmado `Published`.

## Limites e cancelamento

Original, admissions e captures são rechecados antes/depois dos awaits SDK,
em cada batch/linha, no EOF, na imagem pronta, antes de preparar/commit e na
entrega. Espera pelo writer usa o contexto real; cancelamento/deadline não
gera um novo contexto nem renova a autorização.

Read e Write usam Content com limite individual de identificador. Wiki e
references authored preservam strings vazias quando válidas, duplicatas e
integridade, sem quota agregada de páginas/texto/edges. Headers, namespace,
recipe, timestamps, schema, witness e a imagem completa são validados até EOF.
As strings de `expected` conservam a comparação exata de geração/seleção;
não recebem o antigo agregado textual ou um cap substituto. Capturas nativas
e batch SDK mantêm seus controles efetivos. Não há quota fictícia de RSS,
limite serializado por row ou autoridade na chave de recuperação.

## Evidência e consumidores

Source: [adapter Knowledge](../../crates/graphit-knowledge/src/document_republication.rs),
[produtor Data](../../crates/graphit-data/src/knowledge_document_update.rs),
[encoder/witness C8](../../crates/graphit-data/src/knowledge_references.rs).

Fixtures: [Data Native](../../crates/graphit-data/tests/knowledge_document_update.rs),
[phases privadas do mesmo produtor](../../crates/graphit-data/src/knowledge_document_update/tests.rs),
[Captured FILE Knowledge](../../crates/graphit-knowledge/tests/document_republication.rs).
A receipt `.build/rust-knowledge-document-update/validation.json` conserva comandos,
logs, hashes dos artefatos próprios e falhas históricas. Execuções finais: Data
Native05 8/0/0 (104.79s); Knowledge Native03 3/0/0 (70.35s); Data phases unit02
3/0/0 (10.28s); índice/codec unit01 17/0/0 (0.03s). A extensão da mesma fixture
Auth passou 1/0/0 com sete funções filtradas (0.48s), cobrindo WrongWrite e
ProjectId diferente; adiciona zero funções à contagem. A extensão D2 também passou 1/0/0, com sete
filtradas (72.89s), incluindo dois headers atualizados no mesmo request, um
com Some Preserve e outro com None Preserve; adiciona zero funções. O strict
final04 das duas libs
e dois alvos passou; não é um gate SDK global. A revisão estática fria resolveu
writer lifetime e o oracle PreparedReached, com resultado final NoFault.

Falhas históricas incluem callers concorrentes de deadline opcional, import
em construção de outro produtor, erros de fixture corrigidos, enum de erro
grande corrigido com `Box<DeltaRecoveryKey>` em `Published` e falha operacional
de linker durante falta de espaço. Nenhuma dessas tentativas conta como PASS.
Não há qualificação de App/HTTP/global, import lifecycle, desempenho ou
Windows/macOS nesta entrega.

A integração C5 de perguntas documentais usa este adapter público para alterar
uma página não selecionada e observar nova geração; ou mudar whole source/
recipe de uma página selecionada e obter novos digests/parâmetros. O binding
da pergunta, child/model e comportamento HTTP pertencem ao consumidor da
tsk-8fe2; estas fixtures não qualificam App, transporte ou validação ponta a ponta.

O armazenamento preserva schema físico FULL18, leitura até EOF, witnesses, namespace/estado, CAS, causas reais e limites individuais de identificadores, sem contabilidade agregada do corpus.

## Histórico documental no mesmo commit

Os resumos de sincronização são propriedades da Transaction documental real,
com UUID/operação/CAS/recovery preservados. No-op e erro anterior ao commit não
geram evento; Published após falha de entrega conserva o evento já confirmado.
A primeira imagem vazia continua publicada, e o primeiro named anota seu Clone
real. A [seção de histórico](rust_knowledge_document_lifecycle.md#segurança-referências-e-concorrência)
especifica o leitor admitido, ausência/NoCorpus, cutoff, cleanup e limites reais.
Resultados anteriores desta página não qualificam essa extensão.
