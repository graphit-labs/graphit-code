---
type: specification
title: Leitura readonly de Knowledge Native importada
description: Abertura de artifact Knowledge registrado globalmente com admissão Web real, pin SDK e leitura protegida até EOF.
references:
  - target: {type: task, id: tsk-3514}
    relation: supports
  - target: {type: task, id: tsk-8c07}
    relation: implements
  - target: {type: task, id: tsk-2f62}
    relation: uses
  - target: {type: task, id: tsk-58db}
    relation: uses
  - target: {type: task, id: tsk-c8e9}
    relation: uses
  - target: {type: task, id: tsk-41d1}
    relation: derived_from
---

# Leitura readonly de Knowledge Native importada

O integrador fornece uma admissão Web real e a seleção exata de Knowledge
Native já instalada. `graphit_knowledge::imported_knowledge` conserva uma pin
SDK privada e o request original durante abertura, leitura e entrega.

O [instalador Native](../development/native-packages.md) publica e registra o
payload. O reader não instala, cria corpus, materializa branches, repara dados
ou recupera schemas antigos.

## Admissão e entrada

A admissão vem de `RoleIssuer::admit_web_imported_knowledge_read`, com issuer,
caller Web, verifier, provider configurado e captura Installation reais. A
política dedicada é habilitada no servidor e nega por padrão. O Original vem
de `admission.request()`; ID, versão, alias, pathname, metadata e digest não
concedem leitura nem viram PROJECT, USER ou NativeCaller.

```rust
pub struct ImportedKnowledgeInput<'a> {
    pub registry: &'a HubRegistry,
    pub id: &'a ArtifactId,
    pub version: &'a ConcreteVersion,
    pub context_alias: Option<&'a str>,
}
pub struct ImportedKnowledgeReadLimits {
    pub directory: NativeDocumentFileLimits,
    pub index: KnowledgeIndexReadLimits,
}
pub async fn open_imported_knowledge<'a>(
    admission: &'a ImportedKnowledgeReadAdmission<'a>,
    input: ImportedKnowledgeInput<'a>,
    limits: ImportedKnowledgeReadLimits,
) -> Result<ImportedKnowledgeOpen<'a>, ImportedKnowledgeReadError>;
```

ID, ConcreteVersion e Registry usam seus construtores e limites Hub. Alias
presente deve ser não vazio e caber em `index.content.max_identifier_bytes`,
em bytes UTF-8. Erro produz `InvalidLocator`; o texto conserva espaços e bytes
sem trim ou normalização. `directory` limita as capturas físicas na abertura;
as chamadas posteriores reutilizam essas capturas e os limites lógicos Data.

## Seleção e abertura

O Registry é global, conserva o ID e o brand da instalação admitida e seu parent
resolvido por `canonical_directory` corresponde ao owner root capturado. A
grafia configurada não é reescrita. O reader retém uma captura desse owner root
Auth e `HubRegistry::load_captured` lê por sua raiz canônica, com Original e
identidade revalidados antes e depois. Os limites do documento Hub e a validação
do Registry lógico original continuam vigentes, inclusive a igualdade dos roots
lexicais persistidos em entradas GlobalCache. Links observados no parent continuam
rejeitados pela política Hub; comparar somente IDs não admite outro diretório.
O lookup usa Knowledge, ID completo e
ConcreteVersion exatos. A instalação exige target GlobalNative, backend Native,
payload Original com selection_binding None, ausência de alias e entry name,
e original_source com tipo Knowledge, ID e versão iguais à seleção.

O reader conserva o tuple selecionado e compara novamente com o registro atual
nos guards. Desaparecimento ou substituição produz `SourceSelectionChanged`;
erros de leitura ou parsing conservam sua causa.

O [locator local](../architecture/rust-knowledge-import-locator.md) é
`SourceRef.directory()/index.lance`. Para HubCache cujo root é exatamente o parent
lógico do Registry, somente o sufixo já validado é resolvido sob o owner canônico
capturado. O SourceRef persistido e o tuple usado pelos guards permanecem literais;
outras origens não são redirecionadas. Raiz e filho são capturados readonly pelo
Core. O pathname canônico deve corresponder à seleção física assim composta;
links no payload ou em `index.lance` continuam erros. Capturas e
ancestrais são revalidados. `package.json` é envelope, sem leitura no payload.
O caminho OS capturado vira file URL pela biblioteca `url`, sem conversão lossy.

A abertura usa `LanceModuleStore::open` e `knowledge_index::physical_schema()`,
sem branch ou with_version. ConcreteVersion seleciona o artifact e não é branch
nem geração Lance. O reader observa e conserva o snapshot default efetivamente
aberto. Registry não fornece binding imutável entre versão do artifact e geração
SDK; a consistência é da pin observada durante a operação, sem imunidade a GC
ou confinamento contínuo de pathname.

## Resultado e observação

```rust
pub enum ImportedKnowledgeOpen<'a> {
    RegistryAbsent,
    Published(Box<CapturedImportedKnowledge<'a>>),
}
pub struct ImportedKnowledgeObservation<'a> {
    pub producer: ProjectId,
    pub id: &'a ArtifactId,
    pub version: &'a ConcreteVersion,
    pub context_alias: Option<&'a str>,
    pub generation: KnowledgeGeneration,
}
```

RegistryAbsent significa ausência da seleção exata no Registry autorizado,
confirmada novamente antes da entrega. Uma instalação Knowledge válida exige
index.lance: raiz ou componente ausente, captura inválida, schema errado ou erro
SDK são falhas causais. Não existe RegisteredNoCorpus nem tradução para NoCorpus.

CapturedImportedKnowledge possui campos privados para grant, seleção, capturas
e snapshot. Não expõe store, snapshot, clonagem, serialização ou mutação de
autoridade. `observation()` retorna uma referência à metadata acima. A geração
tem physical_branch e inherited_target None; a observação não concede acesso.

## Heads e página

```rust
pub async fn heads(&self, limits: KnowledgeHeadReadLimits)
    -> Result<KnowledgeHeadRead, ImportedKnowledgeReadError>;
pub async fn page(&self, slug: &str, limits: KnowledgeIndexReadLimits)
    -> Result<KnowledgeSnapshotPage, ImportedKnowledgeReadError>;
```

Heads retorna Published com geração e heads owned. Página filtra o slug no mesmo pin e distingue após EOF selecionado:

| Resultado | Significado |
| --- | --- |
| PublishedEmpty | Snapshot válido com zero linhas físicas. |
| PageAbsent | Há linhas físicas, sem documento Active com o slug solicitado. |
| Found { document, focused_source } | Documento integral e proveniência do mesmo registro selecionado. |

Removed selecionado continua validado antes de excluir. Todos os documentos removidos
produzem heads vazios e PageAbsent, conservando a publicação. Slug inválido
produz Data InvalidField("slug"); excesso de bytes, LimitExceeded("identifier_bytes").

O [schema Data](../architecture/rust-knowledge-reference-storage.md) tem 18
campos físicos; KnowledgeDocument mantém 16 campos públicos. Projeção presente
exige witness válido; ausência permanece incompleta. O alias importado não
renomeia namespace, targets, scopes ou contextos persistidos e não altera o
witness. Producer do artifact não vira namespace PROJECT fictício.

Página usa o scanner normal com predicado tipado do slug, valida os registros
selecionados até EOF e extrai documento/proveniência juntos. Duplicatas e
corrupção selecionadas impedem entrega parcial. Corpos alheios não são lidos ou
validados pelo get; a antiga exigência de falha por corrupção não selecionada
foi supersedida. Um probe de metadata de uma identidade distingue corpus vazio
de página ausente sem outro body. Não há segunda consulta global para obter o
focused_source. Namespace/witness ausentes não são fabricados.
Heads continua uma enumeração: projeta oito campos necessários à metadata e
ao witness, valida as linhas do catálogo e não projeta bodies. Seus limites
individuais permanecem os do reader real, sem inventário/quota cumulativa nova.

## Catálogo importado no browser

`POST /api/knowledge/imported/heads` recebe somente a seleção importada:

```json
{"input":{"source":{"kind":"imported_knowledge","artifact":{"id":"vendor/manual","version":"2026.1","context_alias":"manual"}}}}
```

O alias é opcional e literal. A rota usa a sessão opaca, o Origin exato e a
admissão `ImportedKnowledgeReadAdmission` da instalação, com o provider e o Trust
atuais. A seleção não concede autorização PROJECT. O Registry conserva seu
lookup exato de ID/versão e seus guards de instalação e payload Native.

A resposta é `{source, native}`. O `source` repete a seleção literal admitida.
`native.state` é `registry_absent` quando a seleção exata não está registrada, ou
`published` com `generation` e `heads`. Uma publicação vazia conserva sua geração
e retorna `heads: []`. Cada head contém `slug`, `title` e `updated_at`; os heads
são ordenados pelo slug literal. Datas usam o mesmo formato do catálogo PROJECT:
string decimal de nanossegundos assinados desde Unix epoch, ou `null`.

Esse consumidor usa uma única `capture_source(limits)` SourceOnly e percorre
`next_documents()` até `None`. `Some(empty)` continua a leitura. Cada FILE é
validado pelo reader existente; apenas slug, título e data permanecem no catálogo,
enquanto body e demais campos são descartados após cada documento. A captura usa
somente Source e mantém os guards da pin, do Registry, da autoridade e do Original.
O catálogo não requer Model nem NN. Após EOF e após a serialização limitada, a
captura é revalidada antes da entrega. Uma cauda inválida, cancelamento ou perda de
autoridade produz falha causal, sem prefixo saudável de heads.

Os focais HTTP em
`crates/graphit-app/src/browser_auth/tests/imported_knowledge_heads.rs` usam as
fixtures existentes de sessão assinada, publicação Native e Registry. O consumidor
da UI deve usar esse catálogo para enumerar páginas e conservar a seleção
importada ao abrir o leitor de página.

## Guards e causas

Os wrappers Data guarded verificam Original e guard externo antes/depois dos
awaits SDK, por batch/linha, EOF e entrega. O guard revalida Auth, tuple atual do
Registry e capturas. Knowledge também revalida antes da chamada e após resultado.
Uma falha externa tipada fica em um slot privado; o callback Data retorna um
sentinel, e o wrapper devolve a causa externa real. Falhas Data anteriores
conservam sua causa. Não há classificação por Display ou mensagens SDK.

ImportedKnowledgeReadError contém Auth(AuthError), Registry(io::Error),
Capture(io::Error), Data(KnowledgeIndexError), InvalidRegistryScope,
UnsupportedBinding, InvalidLocator e SourceSelectionChanged. Error::source()
conserva causas internas. Cancelamento/deadline são cooperativos no Original;
a recusa AuthError::Request é o contrato Auth existente e não é reclassificada
por uma observação posterior. Durante a leitura, o check do Original em Data
pode observar cancelamento primeiro e conservar Data(Request(Interrupted));
a causa não é substituída por Auth. Não se promete preempção SDK ou limite de RSS.

## Referências da pin importada

`tsk-3514` implementa CapturedImportedKnowledge::references sobre a snapshot
já retida, com a mesma admissão Web/Registry/capturas e causas tipadas. O
[contrato da query importada](rust_imported_reference_query.md) conserva os DTOs
Data, stored namespace/provenance e páginas bounded; o cursor acrescenta digest
streaming da seleção completa, sem autoridade/HMAC. A query tem qualificação
própria Linux: oito funções SDK distintas, um compile-fail separado e
check/strict/fmt/diff aprovados. A prova OwnRead/heads/page desta folha não a
substitui nem recebe essas contagens por herança.

## Qualificação

Em Linux, `native05` passou os sete testes de Knowledge importada. O caso
adicional de cancelamento durante a leitura passou em `cancel01`, preservando
a causa real Data(Request(Interrupted)). Data passou dez casos IndexPage e,
após ampliar o guard terminal, oito ModuleSnapshot. O doctest compile_fail
comprovou que o caller não acessa o snapshot privado.

A suíte final `native07`, incluindo substituição real da captura, passou os
sete casos. `strict07` passou para Knowledge/Data lib e os três targets afetados
com `--no-deps -D warnings`. Após corrigir o produtor Records, o check final
exigiu apenas trocar `err().expect()` por `expect_err()` na asserção da fixture.
Os 25 testes funcionais distintos e o doctest separado qualificam esta folha
em Linux; reruns não são somados. Formatação, diff e os cinco links relativos
das duas páginas técnicas passaram. A revisão de impacto/cobertura e a
conferência Source↔Docs/Wiki aprovaram os contratos desta entrega.

Os logs preservam erros corrigidos da fixture, interrupção por ENOSPC e falha em
um produtor Records. Esses resultados não foram convertidos em PASS histórico.

```text
cargo test --offline --locked -p graphit-knowledge --test imported_knowledge -j1
cargo test --offline --locked -p graphit-knowledge --doc imported_knowledge -j1
cargo clippy --offline --locked -p graphit-knowledge --lib --test imported_knowledge -j1 --no-deps -- -D warnings
```

O `receipt` `.build/rust-imported-knowledge/validation.json` registra
resultados reais e limites. As suítes Data afetadas são verificadas por propósito.
Esta folha não qualifica query FullReferences, FullNoLoss, Browser/IAB, wire App,
todas as plataformas ou validação ponta a ponta.
