---
references:
  - target: {type: task, id: tsk-994a}
    relation: implements
  - target: {type: task, id: tsk-8a17}
    relation: implements
  - target: {type: task, id: tsk-9e41}
    relation: implements
  - target: {type: task, id: tsk-190c}
    relation: implements
  - target: {type: task, id: tsk-fa3d}
    relation: implements
  - target: {type: task, id: tsk-0fb2}
    relation: implements
  - target: {type: task, id: tsk-4e48}
    relation: derived_from
  - target: {type: task, id: tsk-7d75}
    relation: supports
  - target: {type: task, id: tsk-d6f7}
    relation: supports
---

# Store Lance por módulo

O módulo `graphit_data::lance_module_store` oferece snapshots e publicação otimista estrita sobre Lance 14.0.2, com Arrow58.3.0. A página descreve o Source integrado ao Main e as seis fixtures do produtor. A qualificação histórica dos seis testes, Rustdoc e Clippy foi executada com o SDK 13; não constitui evidência da atualização para 14.0.2. A qualificação atual está em [Official Lance integration](../development/lance-upstream.md). A qualificação privada original pertence a `tsk-9e41`. Resultados de execução e qualidade pertencem aos recibos do responsável; qualificações anteriores do SDK não constituem entrega do módulo Task.

O consumidor escolhe um corpus por módulo e fornece seu schema físico completo em Arc<Schema>. A biblioteca compara o schema de cada RecordBatch e o schema carregado do Dataset com esse schema; não escolhe famílias, identidades ou campos do domínio. Source: [lance_module_store.rs](../../crates/graphit-data/src/lance_module_store.rs); fixtures: [lance_module_store.rs](../../crates/graphit-data/tests/lance_module_store.rs).

## Abertura, leitura e publicação

| API | Comportamento implementado |
|---|---|
| LanceModuleStore::create(uri, schema, batches) | Modo Create; lista vazia vira um RecordBatch vazio com o schema fornecido. Prepara a transação e tenta publicá-la sem retries nem autocleanup. |
| LanceModuleStore::create_with_recovery(uri, schema, batches) | Mesmo produtor Create, público; sucesso devolve `(LanceModuleStore, RecoveryKey)` da transação real. O caller conserva ambos antes de suas verificações de entrega. |
| LanceModuleStore::prepare_create(uri, schema, batches) | Valida os batches e prepara a transação Create no SDK; devolve PreparedCreate sem publicar Main. |
| prepared_create.recovery_key()/commit() | Observa a chave real antes da publicação; commit consome a transação uma vez e devolve store/chave ou a causa SDK de criação. |
| LanceModuleStore::open(uri, schema) | Carrega o Dataset existente e verifica schema. Leitura de corpus ausente retorna erro SDK; não cria corpus. |
| store.snapshot() | Copia o locator e retém schema/Dataset por Arc em ModuleSnapshot. |
| snapshot.version()/schema() | Expõe geração SDK e schema observados pelo snapshot. |
| snapshot.scan() | Retorna Scanner Lance sobre Arc do Dataset; o consumidor configura filtro/projeção e consome seu stream. |
| snapshot.prepare_replace(batches) | Exige o schema observado pelo snapshot e prepara uma imagem completa. Ainda não publica a nova imagem. |
| snapshot.prepare_overwrite(schema, batches) | Valida os batches contra o schema explicitamente fornecido, inclusive metadata e largura de campos. Lista vazia retém esse schema em um batch vazio. Usa o mesmo Overwrite e a mesma base SDK do snapshot, retornando PreparedCommit. |
| prepared.commit() | Consome o PreparedCommit e publica com max_retries0 e skip_auto_cleanup true; sucesso retorna outro LanceModuleStore. |

Prepare pode escrever artefatos físicos de dados/transação: ausência de publicação não significa ausência de IO nem rollback desses artefatos. A imagem fornecida para Overwrite é inteira. Cabe ao consumidor incluir todas as famílias, registros e histórico que devem sobreviver; a biblioteca não reconstrói conteúdo omitido.

`prepare_replace` atende consumidores que precisam conservar o schema físico. `prepare_overwrite` atende a publicação de uma nova imagem com identidade ou dimensão diferente, como a troca do modelo de um índice vetorial. Essa operação não converte dados antigos: o consumidor fornece a imagem inteira e o schema pretendido. Batches incompatíveis retornam `SchemaMismatch` antes de preparar a transação. O store retornado pelo commit conserva o schema preparado; um snapshot retido continua com seu schema e sua geração anteriores. Abertura explícita com o schema antigo recusa um head que publicou o novo schema.

Um snapshot continua associado à geração de seu Dataset, mesmo após outro escritor publicar. Seu Arc conserva o handle em memória, mas não é tag, pin ou proteção contra GC. Não habilitar autocleanup nesta API também não impede limpeza externa. Um escritor concorrente que perde a mesma base não é rebaseado automaticamente; a decisão seguinte pertence ao consumidor após observação do resultado.

Exemplo do fluxo em memória com as APIs atuais, não executado como arquivo Markdown. A persistência da chave antes de commit pertence à integração do chamador. A serialização usa a dependência serde_json do módulo; integradores escolhem seu próprio formato Serde.

~~~rust
use std::sync::Arc;
use arrow_array::RecordBatch;
use arrow_schema::Schema;
use graphit_data::lance_module_store::{LanceModuleStore, RecoveryKey, Resolution};

async fn publicar(
    uri: &str,
    schema: Arc<Schema>,
    imagem_completa: Vec<RecordBatch>,
) -> Result<Resolution, Box<dyn std::error::Error>> {
    let store = LanceModuleStore::open(uri, schema).await?;
    let preparado = store.snapshot().prepare_replace(imagem_completa).await?;
    let dados_da_chave = serde_json::to_vec(preparado.recovery_key())?;
    let seguinte = preparado.commit().await?;
    let chave: RecoveryKey = serde_json::from_slice(&dados_da_chave)?;
    Ok(seguinte.resolve(&chave).await?)
}
~~~

## Recuperação e erros

RecoveryKey tem campos privados e getters corpus_uri(), base_version() e transaction_id(); suporta Clone/Eq/Serde. Conserva o locator EXATO usado para abrir/criar, a base transaction.read_version e o UUID transaction.uuid do SDK. Deserializar a chave produz dados de recuperação, não autorização. Resolve rejeita locator diferente antes de IO, incremento de base com overflow e transaction_id vazio; não presume equivalência entre locators nem valida formato UUID adicional.

store.resolve(&key) lê a transação da geração base+1. Não publica nem repete o commit:

| Resultado | Observação |
|---|---|
| Applied { version } | A geração esperada existe e seu UUID coincide com o da chave. |
| Conflicted { version } | A geração esperada existe com outro UUID. |
| Unresolved | A geração estava ausente nessa leitura; um commit ainda em andamento pode publicá-la depois. |
| MissingTransaction | A geração existe, mas não contém transação verificável. É erro, não sucesso ou conflito inferido. |

Applied não afirma que a geração ainda seja o head nem valida efeitos do domínio. Ausência só vira Unresolved para DatasetNotFound/VersionNotFound quando o erro não indica status de commit desconhecido. Outros erros SDK são preservados em StoreError::Sdk; geração observada inesperada retorna InvalidVersion.

CommitFailure conserva recovery_key(), sdk_error() e a cadeia Error::source, inclusive causas SDK de commit desconhecido. Falha na etapa de commit de create é StoreError::Creation contendo esse CommitFailure com UUID, base0 e locator da criação. Falhas anteriores de execute_uncommitted retornam erro SDK sem chave de commit construída; não prometer recovery key para toda falha de criação. SchemaMismatch, CorpusMismatch e InvalidRecoveryKey mantêm suas distinções.

Cancelamento/drop de futures segue o comportamento assíncrono normal. Esta biblioteca não adiciona protocolo de cancelamento/cleanup ou drain. Depois de enviar um commit, perda de acknowledgement ou interrupção pode deixar resultado desconhecido para o chamador; conservar a chave e reobservar não autoriza retry cego nem prova rollback. O teste de lost acknowledgement descarta deliberadamente um sucesso já concluído, não simula crash ou cancelamento durante commit.

## Consumidores e verificação

Auth, claims, fencing de ator, lease e regras de Memory/Task/Knowledge/Dream pertencem aos consumidores de domínio. A geração SDK é uma fence de storage; nenhum locator, schema, chave Serde ou snapshot cria grant de domínio.

Os seis testes executáveis cobrem criação vazia/reopen/leitura ausente; imagem com famílias e snapshot antigo; dois escritores concorrentes; schema incompatível; prepare abandonado/ack descartado com chave serializada; filtro/projeção, UInt64::MAX e chaves inválidas. Seu schema family/id/revision/body é fixture, não schema completo do produto. MissingTransaction e commit desconhecido têm tratamento Source; não há fixture de injeção desses erros nesta unidade.

Os dois casos adicionais de `prepare_overwrite` cobrem troca de metadata com largura igual, troca de dimensão com corpus vazio, schema retornado/reabertura e snapshot antigo; batches incompatíveis, prepare abandonado e conflito de overwrite contra replace conservam recuperação por UUID. Para `FixedSizeList<Float32>`, as fixtures fornecem o schema físico Lance13, cujo filho `item` é nullable; a verificação de valores nulos pertence ao domínio. Não há conversor de schemas nem relaxamento da igualdade nesta API. Esses oito testes do target passaram em Linux; a qualificação de `tsk-190c` e seus logs estão em `.build/rust-module-schema-overwrite/`. A API de armazenamento não qualifica o produtor NativeNN que a consumirá.

Qualificação privada original em Linux: seis testes de integração, um exemplo rustdoc compilado, Clippy estrito e fmt passaram. Os comandos focais Main desta promoção, a partir da raiz do projeto, são:

~~~text
cargo xtask cargo test --locked --offline -p graphit-data --test integration -- lance_module_store:: lance_branch_store::
cargo xtask cargo test --locked --offline -p graphit-data --doc -- lance_
cargo xtask cargo clippy --locked --offline -p graphit-data --lib --test integration -- -D warnings
~~~

A promoção de Source não qualifica o consumidor Knowledge, Task ou Memory. Windows/macOS e consumer journeys precisam de suas execuções próprias. Main Cargo/Lock foram integrados pelo coordenador em `tsk-8dc3`. A tentativa de Clippy Data com `--all-targets` falhou nos imports do teste `wiki_corpus`, cuja entrega Knowledge ainda aguardava seus exports e dependências; esse resultado amplo não passou. Os targets desta promoção passaram com o SDK e cache Main atuais, conforme recibos de `tsk-8a17`.


## Chave real da primeira criação

Integradores que precisam conservar a observação da primeira publicação usam
`LanceModuleStore::create_with_recovery(uri, schema, batches).await`. O método
público retorna o store confirmado e a `RecoveryKey` do SDK. Ele compõe
`prepare_create` e `PreparedCreate::commit` sobre a mesma transação Create.
`create` continua atendendo consumidores de imagens prontas que não precisam
retornar a chave no sucesso.

A chave conserva o locator literal, a base da transação e seu UUID real.
`store.resolve(&key)` pode observar Applied após uma reabertura, inclusive para
um corpus vazio com schema e metadata completos. Isso não afirma que a criação
continue no head nem concede autoridade. O chamador conserva o store e a chave
antes de checar seus próprios controles após a publicação, aguardando o resultado
de um commit iniciado. Esta API não cria Auth, Original, diretórios capturados,
protocolo de drain ou journal de recuperação.

Somente o sucesso e `StoreError::Creation` garantem a chave retornada. Um batch
com schema incompatível é rejeitado antes de criar o corpus. Outras falhas podem
ocorrer depois de efeitos: a validação `from_dataset` acontece depois do commit
e pode retornar sem chave; abandonar a future também pode perder o resultado.
Ausência de chave não prova ausência de efeito. Uma falha de commit mantém
`CommitFailure` com chave e causa SDK; nenhum desses resultados autoriza retry
cego, overwrite do corpus existente ou criação de uma chave artificial.
Materialização de ref e overwrite de branch têm resultados distintos, conforme
seus produtores próprios.

O target externo [lance_module_creation.rs](../../crates/graphit-data/tests/lance_module_creation.rs)
qualifica acesso público e SDK real: payload Unicode/CRLF/NUL/null/vazio e
metadata, chave serializada/reabertura/Applied, corpus estrangeiro, criação
vazia, schema incompatível antes do corpus e Create duplicado sem overwrite.
A falha duplicada registra a fase efetivamente observada; não simula erro de
commit desconhecido ou falha de validação após commit. Os comandos focais são
`cargo xtask cargo test --offline --locked -p graphit-data --test integration -- lance_module_creation::` e
`cargo xtask cargo clippy --offline --locked -p graphit-data --lib --test integration -- -D warnings`.
Resultados e limitações desta interface pertencem à qualificação em
`.build/ast-native-nn-main-creation-key/`. Ela não qualifica bootstrap NativeNN,
Auth do caller, ModelService, integração Main ou extração de todas as linguagens.

Na qualificação original de `tsk-fa3d` em Linux, os três testes passaram em 0,02 s;
a invocação Cargo levou 142,664 s, incluindo espera normal pelo target compartilhado.
O Create duplicado falhou na preparação SDK, antes de uma chave de commit
retornada. Clippy estrito de lib e deste target passou em 25,374 s. Ambas as
invocações passaram na primeira tentativa. Windows/macOS permanecem UNRUN.
Formato focal e três links locais passaram; naquela entrega o corpo
executável de criação e os testes existentes foram preservados. O recibo
`.build/ast-native-nn-main-creation-key/qualification.json` conserva comandos,
logs, duração, hashes e limites observados.

## Preparação separada para validar Source antes do commit

`LanceModuleStore::prepare_create(uri, schema, batches).await` devolve uma
`PreparedCreate` que retém a transação SDK, seu schema e a chave real. A URI é
emprestada da chave, evitando uma cópia redundante. `recovery_key()` permite
conservar a observação antes da publicação; `commit(self).await` consome essa
transação uma vez, com Create, zero retries e sem autocleanup.

Essa separação atende o publisher Knowledge que precisa revalidar a fonte física
sob seu writer entre preparar e publicar. O chamador conserva suas capturas,
Auth e Original, verifica a fonte antes de preparar e antes do commit e aguarda
um commit iniciado antes de avaliar erros posteriores. A API Data não inventa
um guard, callback, autorização ou transação conjunta com o filesystem.
`create_with_recovery` compõe as mesmas operações para os consumidores de uma
imagem pronta que não precisam executar uma verificação entre essas fases.

Preparar pode escrever fragmentos sem publicar Main. Abandonar `PreparedCreate`
não publica, remove esses fragmentos nem promete rollback. Uma criação
concorrente continua sujeita ao CAS real; o perdedor preserva sua chave e causa
SDK quando falha no commit, e não sobrescreve o vencedor.

Os quatro novos casos no target `lance_module_creation` verificam preparação
sem Main, chave/payload após commit e reabertura, concorrência entre duas
preparações, schema inválido antes de IO, abandono, criação vazia e a possibilidade
de o caller recusar commit depois de observar uma mudança no arquivo fonte.
Este último caso comprova o intervalo entre fases; Auth e o guard físico sob
writer do Knowledge são qualificados na integração Knowledge, separadamente.
Os três casos da operação composta permanecem neste mesmo target.

Em Linux, os sete casos atuais de `lance_module_creation` passaram em 0,01 s,
incluindo os três da operação composta; o comando levou 1 min 34 s com a fila
Cargo e compilação. Clippy estrito de lib e deste target passou em 43,92 s.
Resultados novos de `tsk-0fb2` ficam em `.build/rust-lance-prepared-create/`;
os resultados de `tsk-fa3d` acima são históricos. Windows/macOS e a integração
completa Knowledge permanecem sem qualificação nesta unidade Data.

O mesmo target está registrado no runner dos guardrails. Para executar somente
essa cobertura, use `cargo xtask test graphit-data/lance_module_creation`.

## Preparação de Source por stream

`LanceModuleStore::prepare_create_stream(uri, schema, stream).await` aceita o
`SendableRecordBatchStream` real do DataFusion reexportado por
`graphit_data::lance_module_store`, junto com `RecordBatchStreamAdapter` e
`DataFusionError`. O produtor pode enviar `Err(DataFusionError::IoError(error))`
para conservar seu `std::io::Error` original na cadeia de causas do SDK;
`External(Box::new(error))` conserva causas externas que implementam
`Error + Send + Sync`. São os tipos reais do SDK, sem aliases de compatibilidade.
A preparação usa `InsertBuilder::execute_uncommitted_stream` do Lance13 e
valida o schema declarado e o schema físico de cada batch. Não acumula um
`Vec<RecordBatch>` nem cria replay do corpus. Uma imagem vazia exige um batch
explícito de zero linhas; um stream sem batches é erro, preservando o schema
como dado obrigatório do caller.

A API devolve a mesma `PreparedCreate`, depois de consumir o stream até EOF.
Uma falha do produtor após batches válidos retorna erro com sua causa sem
publicar um manifest; fragmentos preparados podem permanecer. A biblioteca
não adiciona drain/cancelamento: o caller deve manter seu produtor e suas
capturas até a conclusão da preparação, revalidar seus controles e aguardar
qualquer commit iniciado. Chave real, Create sem retries e recuperação seguem
os contratos desta página. Nenhum stream ou schema concede autoridade.

Os métodos correspondentes de branch e as fixtures focais são descritos em
[Preparação por stream de branch](rust_lance_branch_store.md#preparação-por-stream-de-branch).
