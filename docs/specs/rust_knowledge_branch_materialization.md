---
type: specification
title: Materialização explícita de branch Knowledge em Rust
description: Criar a ref de uma branch herdada após admissions, geração esperada e validação Full18 até EOF.
references:
  - target: {type: task, id: tsk-2437}
    relation: implements
  - target: {type: task, id: tsk-41d1}
    relation: derived_from
  - target: {type: task, id: tsk-72ce}
    relation: uses
  - target: {type: task, id: tsk-c8e9}
    relation: uses
  - target: {type: task, id: tsk-f02f}
    relation: uses
  - target: {type: task, id: tsk-eb40}
    relation: uses
---

# Materialização explícita de branch Knowledge em Rust

Para consumidores Knowledge que selecionaram uma branch ainda herdada do default
e precisam torná-la física antes de publicar conteúdo diferente. Escopo Native
Linux qualificado: 11 testes novos distintos e uma regressão de fronteira
existente passaram, com clippy focado sem warnings, fmt/diff e revisão fria
NoFault delimitada. A receipt registra o escopo e as falhas históricas. A operação preserva os documentos, referências,
witnesses, rows Removed e a versão SDK capturada. Ela cria a ref pelo SDK existente.

## Jornada

O consumidor obtém a geração selecionada com um admission `ReadKnowledge`, conserva
o mesmo Original e chama materialização com os admissions reais de leitura e
escrita Knowledge. Ambos devem indicar o mesmo projeto, raiz Native, versão e
branch codificada. A geração observada não concede autoridade nem fornece o
diretório de destino: URI e seleção vêm dos admissions.

```rust
pub async fn materialize_native_document_branch(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    expected: &KnowledgeGeneration,
    limits: KnowledgeDocumentUpdateLimits,
) -> Result<KnowledgeBranchMaterialization, KnowledgeBranchMaterializationError>;
```

O adapter Knowledge é fino: delega à API Data
`materialize_native_knowledge_branch` com os mesmos parâmetros, resultado e erro.
Não parseia, copia whole source, prepara H, projeta metadata ou infere namespace.
Para alterar conteúdo depois, o consumidor chama a
[republicação](rust_knowledge_document_republication.md) com a geração física
real retornada, em outra operação autorizada. Primeira criação exclusiva e
[metadata authored](rust_knowledge_authored_first_publication.md) são outra jornada.

## Captura, EOF e resultado

A operação revalida Read e Write Knowledge, cobra os bytes UTF-8 dos textos de
`expected`, adquire o writer cooperativo fora do executor e recaptura o diretório
Knowledge sob os limites Native de ambos os admissions. Ausência confirmada
retorna `NoCorpus`; não cria Knowledge ou adota um diretório alternativo.
Depois do lock captura o pin SDK selecionado e compara toda a geração:
URI, versão u64 SDK, branch física e target herdado.

O scan permanece nesse pin e chega a EOF real antes de no-op ou efeito. O validador
C8 lê a imagem física de 18 campos, inclusive rows Removed, verificando schema,
digests, recipe, timestamps, staleness, raw wiki, estado, namespace nativa e
witnesses. Slugs repetidos em qualquer batch falham. O scan não cria DTOs nem
uma coleção de corpos; conserva apenas slugs limitados para detectar duplicatas.

```rust
pub enum KnowledgeBranchMaterialization {
    Unchanged { generation: KnowledgeGeneration },
    Materialized {
        generation: KnowledgeGeneration,
        inherited: KnowledgeGeneration,
    },
}
```

Default ou branch já física retorna Unchanged somente após comparação e EOF;
não cria nova versão ou transação. Um pin herdado usa `BranchSnapshot::materialize`
para criar a ref na versão **capturada**, mantendo os mesmos dados. O resultado
Materialized inclui a geração de origem e a geração física real. O número u64
pode permanecer igual: a identidade física/herdada da geração muda.

Um escritor SDK não cooperativo pode avançar default depois da captura. Isso
não altera a versão que esta operação materializa. Não há CAS da identidade do
pai, transação global ou promessa de copiar o default mais recente. A versão
capturada e a autoridade do caller continuam explícitas.

## Limites e interrupções

`Read.content` cobra a imagem Full18 completa: páginas, texto UTF-8 agregado,
identificadores e todas as arestas raw, incluindo vazias e duplicadas. Igualdade
é admitida; uma unidade excedente é erro antes da criação da ref. `Read.native`
e `Write.native` limitam os captures reais. `ArtifactStore::new(scope, Write.artifacts)`
recebe `ArtifactLimits`, que já contém `CopyLimits`. A criação
da referência de branch adquire o writer; não lê catálogo, prepara ou copia
conteúdo de instalação, portanto não consome os limites de cópia. Core usa seus
limites FS fixos nesse caminho. Não há overwrite ou
orçamento de conteúdo novo em `Write.content`. Não se afirma enforcement desses
campos pass-through, quota de RSS ou desempenho.

O writer fica retido até a checagem de entrega. Original, admissions e ambos os
captures são rechecados antes/depois dos awaits, por batch/row, em EOF, antes do
SDK e depois da confirmação. Cancelamento e deadline usam o Original recebido;
nenhuma nova requisição renova o tempo. Core conserva `ArtifactError.stage()` e
`kind()`, por exemplo Lock/Interrupted, sem armazenar o io::Error original.

## Erros e efeitos confirmados

```rust
pub enum KnowledgeBranchMaterializationError {
    Data(KnowledgeIndexError),
    ScopeMismatch,
    GenerationChanged { observed: KnowledgeGeneration },
    NoCorpus,
    Materialized {
        generation: KnowledgeGeneration,
        cause: Box<Self>,
    },
}
```

Expected errado falha inclusive no caminho Unchanged. Wrong module, Original
estrangeiro, scopes diferentes, grants ausentes ou revogados não autorizam efeito.
Read não é inferido de Write, nem Write de Read.

Erros SDK `CreationConflict`, `BootstrapIncomplete` e `BootstrapUnknown`
permanecem sob Data/Branch, com causas e observações do produtor real. Um erro
pode vir depois de efeitos físicos do bootstrap; não se inventa rollback,
Unchanged ou recibo de confirmação quando o SDK não retornou sucesso.

Após sucesso SDK, uma falha de cancelamento, Auth ou captura retorna o erro
Materialized com a geração que efetivamente foi confirmada e a causa preservada
por `Error::source`. Esse efeito cria uma ref: não produz `DeltaRecoveryKey`.
Não há retry, retire/delete/recreate ou recuperação por substituição da branch.
Um consumidor autorizado precisa observar o estado atual antes de decidir outro
efeito; uma geração recebida em erro continua sendo observação, sem grant.

## Validação e fronteiras

Fontes: `crates/graphit-data/src/knowledge_branch_materialization.rs` e
`crates/graphit-knowledge/src/branch_materialization.rs`; SDK e codec existentes
em `lance_branch_store.rs` e `knowledge_index.rs`/`knowledge_references.rs`.

As fixtures usam Native PROJECT/Auth/SDK reais: imagem mista Some/None/Removed,
default, branch herdada/física, vazio, old pin, parent independente, expected
completo, corrupção tardia/duplicatas, quotas, grants/scopes, writer concorrente,
SDK externo criando branch ou avançando parent, e confirmação seguida de
cancelamento/revogação/deadline/substituição de captura. O adapter público
Knowledge é seguido pela republicação pública de um FILE capturado/compilado.

```text
cargo test --offline --locked -p graphit-data --test knowledge_branch_materialization -j1
cargo test --offline --locked -p graphit-knowledge --test branch_materialization -j1
cargo test --offline --locked -p graphit-data --lib knowledge_branch_materialization:: -j1
```

Os targets --test não executam os units privados; o --lib focal é obrigatório.
Receipt local `.build/rust-knowledge-branch-materialization/validation.json`
registra comandos, falhas, contagens distintas e limites da qualificação.
Esta folha não qualifica App/HTTP/CLI, Runtime/CoreInventory, C5, imported readonly,
retirada de versão, validação ponta a ponta, outros sistemas operacionais ou desempenho.
A [persistência C8](../architecture/rust-knowledge-reference-storage.md)
e as receipts anteriores conservam seus contratos.
