---
references:
  - target: {type: task, id: tsk-a666}
    relation: implements
  - target: {type: task, id: tsk-4f78}
    relation: uses
  - target: {type: task, id: tsk-8a17}
    relation: extends
  - target: {type: task, id: tsk-3a8e}
    relation: supports
  - target: {type: task, id: tsk-631e}
    relation: implements
  - target: {type: task, id: tsk-7b36}
    relation: implements
---

# Publicação de índice FTS JSON no snapshot Data capturado

Este contrato atende callers Data e Records de `graphit-code`. O Source Data de `tsk-a666` está implementado e Linux-qualified em [fts.rs](../../crates/graphit-data/src/lance_branch_store/fts.rs), com wiring e erros em [lance_branch_store.rs](../../crates/graphit-data/src/lance_branch_store.rs). O contrato foi corrigido na revisão 6 antes do code guard. A terceira execução Source no Linux concluiu **8 PASS, 0 falhas, 0 ignorados, 0 filtrados**, conforme test-third.log (`../../.build/rust-records-memory/search-contract/index-source/test-third.log`; historical local artifact unavailable): testes em 0,37 s e build em 8 min 38 s, incluindo espera por lock. Esses tempos não são benchmark. A primeira compilação falhou por ENOSPC ao produzir archive Auth; a segunda, por E0308 na mutabilidade do argumento SDK; ambas ficam preservadas. A suíte Branch concluiu oito PASS, zero falhas/ignorados/filtrados em 0,16 s, conforme branch.log (`../../.build/rust-records-memory/search-contract/index-source/branch.log`; historical local artifact unavailable). Doctests20 e Clippy afetado também passaram, assim como owned fmt/diff/links/whitespace. Esta é qualificação real do adapter Data no Linux; o estado lifecycle e os checks são consultados na Task, sem antecipar COMPLETED nesta nota.

O precedente [SDK13 FTS](lance13_fts_index_qualification.md), `tsk-4f78` COMPLETED r15, qualificou oito testes SDK, dois testes de erro reforçados, Clippy estrito e formatação no Linux. Essa evidência histórica não substitui a qualificação deste adapter. O protocolo estende Foundation/Branch `tsk-8a17`; Root compôs a dependência de produção `lance-index` 13 e manifests/lockfiles antes dos builds desta folha, após ler o body.

## API e vínculo ao pin

A extensão `tsk-7b36` acrescenta Create/Replace explícitos. Os resultados anteriores
de `tsk-a666`, `tsk-4f78` e `tsk-631e` permanecem históricos, com seus recibos
intactos; não qualificam a extensão. A qualificação da extensão no Linux está registrada abaixo, separada da integração de produto Main.

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JsonFtsIndexMode { Create, Replace }

impl BranchSnapshot {
    pub async fn prepare_json_fts_index(
        &self, column: &str, name: &str, mode: JsonFtsIndexMode,
    ) -> Result<PreparedIndex, BranchError>;

    pub async fn resolve(
        &self, key: &DeltaRecoveryKey,
    ) -> Result<DeltaResolution, BranchError>;
}
impl PreparedIndex {
    pub fn recovery_key(&self) -> &DeltaRecoveryKey;
    pub async fn commit(self) -> Result<BranchSnapshot, DeltaCommitFailure>;
}
```

`JsonFtsIndexMode` e `PreparedIndex` são reexportados por `graphit_data::lance_branch_store`. O preparado retém o snapshot original, a transação SDK, a chave e o `StrictCommit`. A preparação conserva o mesmo `Arc<Dataset>`, schema, URI, localização/branch física e delegate capturados. O builder exige `&mut Dataset` e recebe um clone local do handle SDK capturado; isso não reabre latest nem cria novo contexto. Scan, commit, chave e fence continuam vinculados ao Arc original. Não se recaptura autoridade.

Snapshot herdado retorna `BranchError::InheritedSnapshot` **antes de I/O**; sua materialização pertence ao ciclo Branch existente. A versão alvo é `base.checked_add(1)`; overflow retorna `InvalidVersion` antes da validação/build. Coluna e nome são strings literais entregues ao SDK, nunca interpoladas em filtros ou queries.

O caller conserva admissões Auth emprestadas, guard Core, contexto original, cancelamento/deadline e write lease. Deve revalidar esses vínculos antes de preparar/publicar e conforme o contrato de suas operações. Este adapter não recebe ou renova essa autoridade. O commit é uma chamada pública SDK assíncrona: não há promessa de Auth atomicamente associado à publicação nem rechecks internos durante o SDK. O caller preserva a primeira causa de erro. Este mecanismo não permite reset de tokens/epochs/leases nem certifica proteção contra ABA.

## Validação integral antes do builder

A preparação valida a coluna no pin original antes de `execute_uncommitted`:

1. O campo deve existir e ter tipo Arrow `Utf8`.
2. Um scanner do snapshot projeta a coluna e percorre **todo o stream até EOF**.
3. Cada valor Arrow deve ser efetivamente não nulo, independentemente do nullable flag do schema.
4. `serde_json::from_slice::<serde_json::Value>(value.as_bytes()).map(drop)` valida o valor completo com o parser maduro usado pelo SDK. O Value é transitório por célula e descartado imediatamente; não há retenção do corpus ou framework adicional.
5. Somente após a validação integral o builder pode construir arquivos de índice.

A obrigação do guard é aceitar somente JSON completo compatível com o parser real SDK antes do builder. A fonte SDK usa `from_slice::<Value>` e causa panic em erro. O antigo `IgnoredAny` foi rejeitado: seu skip aceitava overflow numérico como `1e400`, surrogates isolados e nesting profundo que esse parser rejeita. A terceira execução demonstrou as assertions Source: JSON tardio, overflow, surrogates isolados, escape inválido, trailing value e profundidade 256 são rejeitados antes de qualquer arquivo de índice; os casos positivos também passaram. Validação sintática Data não certifica campos de domínio, história, referências, Auth ou seleção current/archive. Não exige objeto Memory: valores JSON aceitos não adquirem semântica Memory pelo parser. Nos adversários executados, o guard impediu que a entrada inválida alcançasse o builder e seu ponto de panic→IO observado no SDK histórico.

| Erro | Contrato |
| --- | --- |
| `JsonIndexColumn(String)` | Campo ausente ou diferente de Utf8; sem construção/publicação. |
| `NullJsonIndexValue { column, row }` | Valor Arrow nulo observado antes do builder. |
| `InvalidJsonIndexValue { column, row, cause }` | Causa `serde_json::Error` preservada; payload não incluído no erro. |
| `JsonIndexReplacementMissing { column, name }` | Replace sem o nome alvo no inventário completo do mesmo pin; falha antes do builder. |
| `JsonIndexReplacementIncompatible { column, name }` | Replace com binding, tipo, política física ou seleção incompatíveis; falha antes do builder. |
| `SchemaMismatch` | Batch projetado incompatível com a representação Utf8 esperada. |
| `Sdk(...)` | Causa SDK de scan, build ou leitura preservada. |

`row` é ordinal zero-based acumulado entre batches, não ID persistente nem ordem estável do corpus. Erro de scan ou entrada tardia invalida toda preparação; não há retorno parcial, publicação de prefixo ou saudável vazio. A árvore Value é descartada após cada célula; a API não oferece quota de corpus ou prova de memória residente.

## Criação, substituição explícita e commit estrito

O analyzer aprovado por Root é JSON/simple, lowercase habilitado, stemming/stopwords/ascii folding desabilitados e `max_token_length(None)`. Não há filtro implícito de 40 bytes. O SDK é Lance/Lance Index 13.0.0 conforme a composição do projeto.

O builder usa coluna/nome explícitos e `IndexType::Inverted`, com os mesmos
`InvertedIndexParams` aprovados. `JsonFtsIndexMode::Create` usa `replace(false)` e
`removed_indices: []`. A verificação SDK de nome usa os metadados completos do
manifesto capturado: colisão na mesma ou outra coluna permanece `Sdk(Error::Index)`,
nunca NoChange ou substituição automática. CreateIndex isolada não garante
unicidade de nome; o protocolo não pula essa verificação.

`JsonFtsIndexMode::Replace` atualiza um índice existente, não é upsert. O adapter
lê o inventário completo com `read_manifest_indexes` sobre o mesmo Dataset,
manifesto e localização do pin capturado. Nome ausente retorna
`JsonIndexReplacementMissing`. Nome ligado a outro campo, índice não FTS, binding
incompatível, política física diferente de JSON/Row ou inventário ambíguo retorna
`JsonIndexReplacementIncompatible`, antes do builder.

Vários segmentos físicos do mesmo nome, campo e política são válidos. Todos os
segmentos reais do nome alvo entram em `removed_indices`. Nomes FTS concorrentes
na mesma coluna ou metadata que impede classificar seguramente a seleção são
incompatíveis. Outros nomes e colunas permanecem intactos. `IndexDetails`,
`load_segment_details` e `load_segment_params` verificam binding e política física;
nome ou metadata declarativa isolados não os substituem. Erros reais SDK de leitura
ou integridade conservam sua causa e não viram Missing.

Replace não acrescenta um scan do corpus à validação JSON integral existente.
Após validar os valores e o inventário, o mesmo builder usa `replace(true)` e a
mesma política física. `execute_uncommitted` escreve arquivos sem publicar
manifesto. O adapter verifica `IndexMetadata.dataset_version == base` e prepara
`Operation::CreateIndex` com a metadata nova e a remoção correspondente ao modo.
Replace publica a metadata nova e remove os segmentos observados do alvo na mesma
transação. O UUID da transação, diferente do UUID do índice, está disponível por
`recovery_key()` antes do commit. Não há recaptura de latest, GC ou rollback.

`commit(self)` usa o `Arc<Dataset>` capturado e `StrictCommit` existente: localização/branch exatas, manifesto em **base + 1**, delegate original, `max_retries(0)` e `skip_auto_cleanup(true)`. As flags/protocolo de predecessor continuam os do handler existente. Vencedor estrangeiro ou preparação concorrente não autoriza rebase, retry, adoção de outra transação ou mudança do conteúdo vencedor.

Sucesso retorna novo `BranchSnapshot` sobre o dataset publicado. O pin anterior conserva a versão; main e named ficam isolados. A geração muda somente para publicar índice derivado: linhas, história e marcador de referências não mudam. Arquivos uncommitted podem permanecer após falha; não há promessa de rollback, limpeza de órfãos ou repair.

## Falha e recuperação existente

`DeltaCommitFailure` conserva `DeltaRecoveryKey` e causa SDK, disponíveis por `recovery_key()`, `sdk_error()` e `Error::source`. A chave vincula URI do corpus, branch física, versão base e UUID da tentativa. É identidade de recuperação, não autoridade.

`BranchSnapshot::resolve` reutiliza a resolução Branch. Receiver de corpus/branch errados retorna `RecoveryBinding`; snapshot herdado, `InheritedSnapshot`; chave inválida, `InvalidRecoveryKey`. A observação é da transação em base+1:

| `DeltaResolution` | Significado delimitado |
| --- | --- |
| `Applied { version }` | A transação observada na versão alvo tem o UUID da tentativa. |
| `Conflicted { version }` | A versão alvo contém transação com outro UUID. |
| `Unresolved` | Ausência definitiva permitida pelo delegate, sem prova de Applied; não autoriza replay. |

Erro indeterminado conserva a causa SDK. Manifesto sem transação retorna `MissingTransaction`, não ausência segura. Resolve não altera corpus, limpa arquivos ou adota vencedor. O caller decide recuperação sob autoridade/política reais; não há novo registry, ledger, resolver inicial ou framework de transações.

## Inspeção JSON FTS no pin capturado: `tsk-631e`

A API readonly em [fts_state.rs](../../crates/graphit-data/src/lance_branch_store/fts_state.rs) está Linux-qualified: seis testes funcionais PASS, strict próprio exit 0 e owned fmt/diff/12 links verificados. Essa qualificação é do inspector Data, sem cobertura Memory ou antecipação de lifecycle Task. `JsonFtsIndexState` é reexportado por `lance_branch_store`:

```rust
pub enum JsonFtsIndexState { Missing, Incompatible, Ready }

impl BranchSnapshot {
    pub async fn json_fts_index_state(
        &self, column: &str,
    ) -> Result<JsonFtsIndexState, BranchError>;
}
```

Essa observação usa o mesmo Dataset/Arc, versão, schema e branch do snapshot, inclusive Main, named ou inherited. Não materializa branch, constrói índice, publica manifesto, repara, limpa ou faz refresh. Coluna ausente ou diferente de Arrow Utf8 retorna `JsonIndexColumn`; não é índice Missing nem corpus ausente.

| Estado/erro | Significado delimitado |
| --- | --- |
| `Missing` | Nenhum índice FTS correspondente à coluna foi declarado no manifesto capturado. Índices não FTS ou de outra coluna não tornam esse estado Ready. |
| `Incompatible` | Metadados/seleção incompatíveis, índices lógicos concorrentes, ausência de segmentos utilizáveis ou política física diferente. Não autoriza escolher um índice arbitrariamente. |
| `Ready` | O SDK seleciona segmentos físicos coerentes com a coluna e a política JSON esperada no pin. É observação, não grant ou certificado de cobertura Memory. |
| Erro | Falha SDK de leitura/decodificação conserva a causa; nunca se converte em Missing ou Ready. |

O adapter lê os metadados completos com `read_manifest_indexes` sobre ObjectStore, manifesto e localização do mesmo Dataset. `IndexDetails::supports_fts`, field ID e keyed fields ligam o índice à coluna real; `load_indices` filtrado ou nome isolado não substituem esse inventário. Vários segmentos do mesmo nome lógico são permitidos; nomes lógicos concorrentes na mesma coluna são Incompatible. A granularidade deve ser `DocumentGranularity::Row`.

`load_segment_details` verifica a coerência. `load_segments(dataset, column, Row)` precisa selecionar segmentos utilizáveis, não uma seleção None/vazia; cada UUID selecionado deve corresponder ao inventário validado, com campos e nome coerentes. `load_segment_params` lê a política **física** de cada segmento. O tokenizer JSON não é inferido de `index_details` ou do nome.

A comparação usa `InvertedIndexParams` e `PartialEq` públicos: Row, JSON/simple, lowercase=true, stemming/stopwords/ascii folding=false e max token length None, como no publisher `tsk-a666`. O diagnóstico confirmou params físicos com formato serializado None: o campo é omitido pelo serde SDK. Isso não é observação de formato físico Some. Antes da comparação, tanto params físicos quanto política esperada recebem o mesmo `format_version(physical.resolved_format_version())` pelo setter público. Formato de postings é detalhe físico SDK, separado da política de tokenizer; não há parser/serde custom nem relaxamento dos outros parâmetros.

Esse seam impede que um consumer trate ausência ou inutilidade do índice como FTS JSON saudável: o fallback flat simple-text SDK não interpreta triplets JSON com a mesma semântica. Ready não fornece Auth, guard Core ou contexto original, não certifica cobertura pós Memory merge/delete e não entrega Memory search. O caller mantém suas admissões/rechecks e seus fixtures. A leitura não cria wrapper de query nem refresh arbitrário.

Os [fixtures de inspeção](../../crates/graphit-data/tests/lance_fts_index_state.rs) usam datasets SDK temporários reais. A terceira execução focal (`../../.build/rust-records-memory/search-contract/index-state/test-third.log`; historical local artifact unavailable) concluiu **exit 0, 6 PASS/0 falhas**, runtime 0,04 s e compilação de 1 min 24 s; os tempos não são benchmark. Ela incluiu as seis fixtures atuais, sem quarto replay: BTree real na mesma coluna Utf8 retornou Missing; publicação JSON de corpus vazio teve seleção SDK Ready. A construção ListElement sobre Utf8 foi rejeitada pelo SDK antes da publicação, sem efeito e sem fabricar metadados ListElement. A validação de entrada também rejeita List<Utf8> por tipo; essa forma não foi fixture executada. A coluna UInt64 e a coluna ausente retornaram `JsonIndexColumn` nas fixtures. Erros físicos nos stores TempDir próprios conservaram as causas. Seleções defensivas None/vazias e combinações de granularidade não são declaradas exercitadas.

A primeira compilação terminou exit 101 por E0599/import de trait e uso child deprecated. A segunda execução teve **1 PASS/3 FAIL** por comparação que não normalizava o formato de postings em ambos os lados; o diagnóstico isolado teve **1 FAIL** e confirmou os demais parâmetros. A normalização de ambos pelo setter público passou na terceira execução. Uma quarta tentativa iniciou prematuramente em fila no lock próprio e foi encerrada exit 143, com log preservado; nenhum processo alheio foi alterado. Não houve replay Branch/index-store/SDK/workspace: o comportamento desses helpers não mudou. A primeira execução strict própria do módulo `lance_fts_index_state`, no target/janela então combinados, concluiu **exit 0**, conforme `.build/rust-records-memory/search-contract/index-state/clippy-first.log`. Build de 6 min 48 s inclui fila e não é benchmark. O receipt `.build/rust-records-memory/search-contract/index-state/validation.json` acompanha logs, resultados, hashes e limites dessa qualificação. O inspector tem sua própria evidência Source6/strict0; não a herda dos receipts `a666`/`4f78`. Outros sistemas operacionais e cobertura Memory continuam sem prova; o estado lifecycle é consultado na Task.

Comandos focais, usando a configuração canônica do checkout:

```sh
cargo xtask cargo test --offline --locked -p graphit-data --test integration -- lance_fts_index_state::
cargo xtask cargo clippy --offline --locked -p graphit-data --lib --test integration --no-deps -- -D warnings
```

## Leituras e qualificação afetada

O consumidor usa `BranchSnapshot::scan()` e FTS SDK real. Leitura/query não
constrói nem atualiza índice. A API não oferece wrapper de query, parser Boolean,
cursor ledger, collapse/top-k Memory ou dependência AI lexical. `load_indices`
é filtrado; Replace exige o inventário completo do mesmo pin. Records compõe
Ensure/Missing com Create, Ensure/Ready como observação readonly e Refresh/Ready
com Replace explícito. Essa composição e seu ingresso MCP/CLI têm qualificação própria.

As fixtures usam datasets temporários Data reais, nunca stores Graphit. As oito passaram na terceira execução Linux; o mapa abaixo resume sua cobertura:

| Cobertura | Evidência executada |
| --- | --- |
| F1 / P1,P3,P5–P7 | Main/named materializada; prepare não avança; chave antes de commit; base+1; query SDK; resolve Applied; dados/pin anterior/main intactos. |
| F2 / P1–P4 | Colisão mesma/outra coluna; índice estrangeiro preservado; schema errado, null efetivo e bad JSON tardio rejeitados antes build/publicação. Adversários incluem overflow numérico, lone high/low surrogate, escape inválido e profundidade 256. |
| F3 / P5–P7 | Dois preparados na mesma base e append estrangeiro vencedor; loser conserva causa/chave sem rebase; UUID/receiver binding; Applied/Conflicted/Unresolved e winner intacto. |

O oitavo teste Source positivo cobre pares válidos de surrogate, chaves duplicadas ordinárias com comportamento last-wins e nesting normal. Esses oito casos Source são distintos dos oito testes SDK históricos de `tsk-4f78`; não houve replay dessa qualificação SDK para esta correção. Todos os oito passaram em `test-third.log`; a suíte Branch também passou seus oito testes em `branch.log`. Os 20 doctests e Clippy afetado também passaram; não houve replay de Source8/Branch8 para essas verificações.

Acceptance: P1–P4 `chk-361ea2fe8aaa`; P5–P6 `chk-6223a912d60b`; P4/P7 `chk-6a17b9d29a97`; docs/suítes `chk-d8cec2fcca53`. Testes: `chk-588107156057`, `chk-beb76a7d6e6c`, `chk-de9f4d5d80f1`. As evidências de execução estão registradas abaixo. Os resultados formais dos checks e lifecycle pertencem ao coordenador e devem ser consultados na Task; esta nota não os atualiza.

## Comandos e qualificação Linux

Na raiz do checkout, com a configuração canônica do projeto:

```sh
cargo xtask cargo test --offline --locked -p graphit-data --test integration -- lance_fts_index_store::
cargo xtask cargo test --offline --locked -p graphit-data --test integration -- lance_branch_store::
cargo xtask cargo test --offline --locked -p graphit-data --doc
cargo xtask cargo clippy --offline --locked -p graphit-data --test integration -- -D warnings
rustfmt --edition 2024 --check \
  crates/graphit-data/src/lance_branch_store/fts.rs \
  crates/graphit-data/src/lance_branch_store.rs \
  crates/graphit-data/tests/lance_fts_index_store.rs
```

Teste novo: `crates/graphit-data/tests/lance_fts_index_store.rs`. O adapter está Linux-qualified com Source8/Branch8/doctests20/affected strict Clippy exit 0 e owned fmt/diff/links/whitespace PASS. Os doctests (`../../.build/rust-records-memory/search-contract/index-source/doc.log`; historical local artifact unavailable), com resultado exit 0 (`../../.build/rust-records-memory/search-contract/index-source/doc-result.json`; historical local artifact unavailable), incluem quatro casos compile/runtime e 16 compile_fail: 20 PASS, zero falhas/ignorados/filtrados. Rustdoc levou 35,56 s e compilação merged 35,22 s. O Clippy afetado (`../../.build/rust-records-memory/search-contract/index-source/clippy.log`; historical local artifact unavailable), com resultado exit 0 (`../../.build/rust-records-memory/search-contract/index-source/clippy-result.json`; historical local artifact unavailable), levou 10 min 17 s incluindo espera por lock. Não houve replay de Source8/Branch8, nem esses tempos são benchmark.

O receipt consolidado previsto é `.build/rust-records-memory/search-contract/index-source/validation.json`; o primary o produz preservando as duas falhas históricas ENOSPC/E0308. Não se exige gate amplo de workspace. A revisão histórica de `tsk-a666` confirmou a assinatura então vigente, EOF, criação sem substituição e tipos Branch. O contrato `tsk-7b36` acrescenta Create/Replace explícitos; commit, Original, CAS, recuperação e obrigações do caller permanecem inalterados. A qualificação da extensão está registrada abaixo, sem substituir os recibos históricos. Qualificação Data não declara fechamento Task por esta transcrição nem entrega Memory search. Windows/macOS UNRUN; os resultados Linux não provam esses sistemas ou produto Memory.
