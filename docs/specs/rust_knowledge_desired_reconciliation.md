---
title: Reconciliação da seleção completa Knowledge em Rust
type: spec
references:
  - target: {type: task, id: tsk-69f2}
    relation: implements
  - target: {type: task, id: tsk-f5a7}
    relation: implements
  - target: {type: task, id: tsk-3f7e}
    relation: uses
  - target: {type: task, id: tsk-d523}
    relation: uses
  - target: {type: task, id: tsk-943a}
    relation: uses
  - target: {type: task, id: tsk-e25e}
    relation: uses
  - target: {type: task, id: tsk-93e8}
    relation: uses
  - target: {type: task, id: tsk-9983}
    relation: uses
  - target: {type: task, id: tsk-0225}
    relation: uses
  - target: {type: task, id: tsk-cd410}
    relation: preserves
  - target: {type: task, id: tsk-852b}
    relation: integrates
  - target: {type: task, id: tsk-6dec}
    relation: refined_by
  - target: {type: task, id: tsk-c9e2}
    relation: refined_by
  - target: {type: task, id: tsk-6ded}
    relation: refined_by
---

# Reconciliação progressiva Knowledge

A API produtiva é:

```text
reconcile_knowledge_source_changes(original, read, write, config,
                                  kind_on_absent, changes, on_file, limits)
  -> Result<KnowledgeDesiredReconciliationResult,
            KnowledgeDesiredReconciliationError>
```

`changes = None` revisa a seleção configurada; um conjunto não vazio processa
somente arquivos e remoções afetados. Um conjunto vazio recupera os slugs já
publicados para trabalho semântico, sem recapturar ou parsear fontes físicas.
O callback recebe apenas o slug de cada arquivo confirmado, inclusive quando
sua identidade/hash/receita permite reutilização sem commit.

Read e Write Knowledge devem pertencer ao mesmo Original, PROJECT, runtime,
versão e branch. Config deve usar a mesma brand. Os controles são verificados
antes dos efeitos e nos produtores. Targets authored são dados literais;
referenciá-los não concede permissão. Diretórios de controle são preparados em
worker aguardado antes da captura Source, conservando a causa de I/O original.

A captura é de metadata; preparação e publicação são por arquivo. A captura de
delta usa a seleção Core retida dos paths reais, sem ler/hashar payloads intactos
fora dessa cobertura. Diretórios, paths removidos e controles de seleção usam as
mesmas regras e o walker existentes, sem outro inventário paralelo. O primeiro
arquivo cria o índice quando ausente; os seguintes reconciliam sua origem na
geração observada. CAS, RecoveryKey, branches herdadas e efeitos conhecidos
pertencem aos produtores Data existentes. Não há escolha explícita de um modo
full antigo, adoção silenciosa após conflito ou segundo pipeline de publicação.

O produtor preserva datas por slug e aplica a receita de compilação real.
SHA/receita iguais permitem pular o parser; troca de política relevante volta a
preparar o arquivo. Source conserva bytes integrais. Mapping, referências,
fingerprints, FULL18/C8 e validade dos dados usam os codecs ativos. Limites são
por arquivo, documento, campo ou requisição, sem saldo artificial do corpus.

O EOF autoriza reconciliar origens ausentes na cobertura da seleção capturada.
Em delta, a ausência de um FILE fora dos paths admitidos não autoriza remoção ou
transferência de seu slug. Remoção não apaga linhas
estrangeiras nem transfere slugs de outra origem ainda selecionada. Rename de
slug estável é aceito quando a origem anterior saiu da seleção genuína.

Cada commit pode ser consultado imediatamente. Uma falha posterior devolve
`Published { publication, pending_paths, cause }`, conservando a última geração
confirmada e somente os arquivos ainda pendentes. Auth, Request, Source,
Mapping, Authored, Projection, DataImage e os erros dos produtores mantêm objetos
causais tipados; não são classificados por mensagens de fornecedor.

O background semântico é independente, selecionado por slugs e confirmado em
lotes de arquivos. Indisponibilidade do modelo não anula Source/FTS nem obriga
reparse dos arquivos concluídos. A troca incompatível de espaço vetorial segue
a reindexação normal; nenhum vetor incompatível entra no leitor atual.

Ver [publicação por arquivo](rust_knowledge_source_stream.md) e
[Foreground](../architecture/rust-knowledge-foreground.md). Os testes protegem
prefixos publicados, delta/rename, metadados e referências, autorização, limites,
fontes inválidas e recuperação; testes exclusivos da atomicidade do corpus e
hooks de fases do mecanismo eliminado são retirados.
