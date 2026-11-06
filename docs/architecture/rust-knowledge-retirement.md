---
references:
  - target: {type: task, id: tsk-820f}
    relation: specified_by
  - target: {type: task, id: tsk-4ea3}
    relation: specified_by
  - target: {type: task, id: tsk-86ab}
    relation: implements
  - target: {type: task, id: tsk-86ec}
    relation: refined_by
  - target: {type: task, id: tsk-4a63}
    relation: uses
  - target: {type: task, id: tsk-669e}
    relation: specified_by
  - target: {type: task, id: tsk-4148}
    relation: specified_by
  - target: {type: task, id: tsk-69f2}
    relation: derived_from
  - target: {type: task, id: tsk-d581}
    relation: derived_from
  - target: {type: task, id: tsk-1e53}
    relation: uses
  - target: {type: task, id: tsk-2515}
    relation: relates_to
  - target: {type: task, id: tsk-1c28}
    relation: relates_to
  - target: {type: task, id: tsk-a51f}
    relation: uses
  - target: {type: task, id: tsk-9d92}
    relation: relates_to
  - target: {type: task, id: tsk-a437}
    relation: uses
  - target: {type: task, id: tsk-1f4a}
    relation: uses
  - target: {type: task, id: tsk-2ba9}
    relation: uses
  - target: {type: knowledge, id: Named_Lance_branch_retirement}
    relation: uses
  - target: {type: knowledge, id: Rust_maintained_project_inventory}
    relation: uses
  - target: {type: knowledge, id: Project_record_admission}
    relation: uses
  - target: {type: knowledge, id: Rust_daemon_memory_and_lifecycle_contract}
    relation: uses
  - target: {type: memory, id: 01M4NY3P8F0Q1CNZ7HP05NNQSY}
    relation: uses
  - target: {type: memory, id: 01M4KWH3MAC801K7E9GJDVSHKM}
    relation: uses
---

# Retirada explícita de versão Knowledge

Knowledge mantém documentos, histórico funcional e embeddings compatíveis em
`knowledge/index.lance`, por PROJECT. Versões usam branches nativas do mesmo
dataset. Retirar uma seleção Knowledge é um único efeito nativo sobre essa
branch, incluindo suas projeções documentais e vetoriais. Não há um segundo
corpus semântico nem uma segunda operação de retirada de embeddings.

Fechar o daemon, estacionar um projeto, encerrar consumidores, retirar uma branch
e remover uma versão do inventário são operações distintas. Knowledge não
autoriza apagar Memory, Task ou AST, nem altera os fatos compartilhados do Core.

## API e autoridade

A API produtiva está em `crates/graphit-data/src/knowledge_index.rs`:

```text
retire_native_knowledge_index(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    limits: KnowledgeRetirementLimits,
) -> Result<KnowledgeBranchRetirement, KnowledgeRetirementFailure>
```

A admissão deve ser `ProjectWriteOperation::Knowledge`, com a permissão efetiva
`MutateModule(Knowledge)`. O produtor revalida a autoridade e exige o mesmo
Original admitido; igualdade de deadline, ProjectId ou URI não substitui essa
prova. Scope, corpus e branch vêm da admissão e do `VersionContext` capturado,
sem selectors posteriores para trocar o destino.

`KnowledgeRetirementLimits` contém somente os limites existentes de entrada e
captura física (`native`) e publicação (`artifacts`). A retirada não varre
linhas, carrega vetores ou executa modelo. Usa o writer cooperante existente,
captura o target físico e chama `BranchSnapshot::retire_named`.

Default é recusado. Uma seleção herdada não é materializada para depois ser
retirada. Ausência exata do corpus retorna `NoCorpus`; uma referência ausente em
corpus existente é uma observação herdada e não equivale a ausência do corpus.
Erros de autorização, leitura, corrupção ou `NotFound` genérico não viram sucesso.

## Resultado e falhas

`KnowledgeBranchRetirement` contém `NoCorpus` ou
`Retired { generation: KnowledgeGeneration }`. A geração é observada antes do
efeito. `KnowledgeRetirementFailure` preserva a causa por `error()` e eventual
efeito confirmado por `confirmed()`, com a cadeia `Error::source()` original.

| Resultado observado | Resposta |
| --- | --- |
| SDK concluiu e rechecagem passou | `Ok(Retired { generation })`. |
| SDK concluiu e rechecagem posterior falhou | Causa posterior e `confirmed() = Some(Retired { generation })`. |
| SDK falhou | Causa SDK e nenhuma confirmação fabricada. Efeitos podem ser incertos. |

Documentos e vetores pertencem à mesma branch: não existe sucesso documental
seguido de falha em uma retirada semântica independente. Um conflito de tag no
SDK deve preservar a branch inteira. Referências, filhos, tags e snapshots
utilizados seguem as garantias do Lance; ausência de confirmação não prova
ausência de efeito.

## Ordem, concorrência e cancelamento

1. Validar classe, autoridade, Original e versão; recusar default.
2. Observar o corpus autorizado sem criar diretórios ou converter erro em ausência.
3. Adquirir o writer Core e recapturar/revalidar o corpus sob esse writer.
4. Capturar o target físico e revalidar nas fronteiras do SDK.
5. Retirar a branch uma vez, mantendo o writer até a rechecagem final.
6. Preservar o resultado e a causa reais, sem replay ou rollback automático.

A espera do lock usa `spawn_blocking`, permitindo que o executor progrida o
holder. Cancelamento/deadline não autoriza outra seleção nem descarta um efeito
já confirmado. A captura física revalida o diretório do corpus após o efeito;
não exige que o manifest da própria branch retirada continue existindo.

O SDK pode recusar descendentes/tags ou falhar durante cleanup. O caller para e
aguarda somente os produtores que realmente possui antes de retirar seus dados.
`Arc<Dataset>` sozinho não garante que arquivos permaneçam presentes. Não há
ledger adicional de leitores, reserva global, framework de quiescence ou deleção
em Drop.

## Consumidor App

`crates/graphit-app/src/knowledge_retirement.rs` compõe
`retire_knowledge_selection` com o `ProjectSupervisors` já retido pelo caller.
Exige `Configure` e autorização Knowledge do mesmo caller antes de
`stop_selection(A)`. Após o join, revalida Original, Configure e Knowledge antes
da única chamada Data.

Falha no join impede a retirada. Cancelamento ou prazo durante o join ainda
aguarda cleanup e bloqueia o efeito na rechecagem. Drop do futuro deixa o owner
no registro recebido, para que o caller continue o encerramento.

O resultado contém somente `index`. As fases são `Admission`, `Stop` e `Index`;
o getter `index()` da falha conserva a confirmação real da operação Data. O
componente não remove inventário Core nem afirma uma transação entre stores.
A API Data não depende de gate Linux; o consumidor App inspecionado permanece
sob `cfg(target_os = "linux")`. Isso não comprova execução Windows/macOS.

## Testes atuais e evidências históricas

`crates/graphit-data/tests/knowledge_retirement.rs` usa Auth, Core, publicação e
SDK reais. Os seis casos documentais existentes preservam autoridade, Original,
recusa de default/herdada, ausência exata, limites, permissões, lock, cancelamento,
prazo e causas SDK. O caso named agora publica e observa o vetor no próprio
`index.lance` antes da retirada; default, sibling e outro corpus permanecem
inalterados. Um caso adicional protege documentos e vetores juntos contra
conflito de tag.

Os cinco casos históricos que exigiam uma retirada independente de embeddings
foram removidos: verificavam um mecanismo eliminado. Suas antigas aprovações são
histórico, não evidência do contrato unificado. Os testes privados causais de
`knowledge_index::retirement_tests` continuam protegendo efeito confirmado e
causa SDK diante de cancelamento posterior.

Para a fixture reconciliada, executar somente:

```text
cargo xtask cargo test --locked --offline -p graphit-data --test integration -- knowledge_retirement::index_retirement_
```

A execução atual deve ser registrada com seus próprios resultados. Evidências
inalteradas podem ser reutilizadas; esta revisão documental não declara nova
aprovação, instalação do produto ou qualificação nativa de outra plataforma.

Contratos relacionados:

- [Retirada de branch Lance](../specs/rust_lance_named_branch_retirement.md).
- [Inventário de projetos](../specs/rust_project_inventory.md).
- [Admissão PROJECT](../specs/project_record_admission.md).
- [Knowledge documental e semântico](rust-knowledge-semantic-data.md).
- [Memória e lifecycle do daemon](../specs/rust_daemon_memory_lifecycle.md).
