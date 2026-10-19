---
type: architecture
title: Projeção documental Knowledge pura em Data Rust
description: Valores authored emprestados, member rows limitadas e contexto fornecido para consumidores Knowledge.
references:
  - target: {type: task, id: tsk-0225}
    relation: implements
  - target: {type: task, id: tsk-cfa56}
    relation: derived_from
  - target: {type: task, id: tsk-31f2}
    relation: uses
  - target: {type: task, id: tsk-95fe}
    relation: relates_to
  - target: {type: task, id: tsk-dff0}
    relation: relates_to
---

# Projeção documental Knowledge em Data Rust

O módulo `graphit_data::knowledge_document_projection` recebe dados authored e
observações de compilação, valida entradas e prepara rows emprestadas para os
consumidores Knowledge. Não lê arquivos, publica conteúdo ou decide permissões.
O compilador documental e os produtores de dados têm seus contratos próprios.

A API está disponível no módulo público Data principal. A integração dos
consumidores Knowledge e da persistência continua em suas entregas próprias.
A integração principal foi validada em Linux com sete testes funcionais, três
doctests e Clippy sem avisos. Outras plataformas não foram executadas nesta unidade.

## Uso pelo consumidor

1. Fornecer o `SnapshotContext` real e conservar os owners dos textos/targets.
2. Preencher `AuthoredPageFields` e `CompilationObservationFields`. Os campos
   obrigatórios precisam estar presentes e os paths/slug não podem ser vazios.
3. Informar máximos positivos do corpus e texto em
   `KnowledgeDocumentProjectionLimits`, então chamar `prepare`.
4. Usar `rows(context)` e `supplied_context(context)` com o mesmo request.
   Cancelamento e timeout originais continuam aplicáveis; contexto diferente
   retorna `DifferentRequest`.
5. Encaminhar a projeção ao consumidor de staleness/reference da folha `tsk-95fe`.
   O consumidor usa os kernels reais `ReferenceGraph` e `assess`, com clock explícito.

`prepare` retorna `PreparedKnowledgeDocumentRows<'data,'context>`. Textos e grupos
referenciam seus inputs originais; descritores temporários podem ser descartados
quando os owners desses dados permanecem vivos. O contexto fornecido precisa
continuar vivo. O slice de rows empresta a projeção e não pode sobreviver a ela.
`ResolvedTargets::as_slice()` conserva o lifetime original do grupo, permitindo
adaptar `PageReferences` sem criar um Vec temporário de targets que escape.

`MemberRow` contém header em `Box<DocumentHeader>`, targets com índice/ordinal e
observações. Headers têm os valores validados e o grupo original. A ordem é header,
seus targets na ordem fornecida, próximos documentos e finalmente observações.
O header usa Box para manter slots pequenos, sem contabilidade artificial de
capacidade/memória. A enum é Clone; descriptors Header/ResolvedTargets são Copy.

## Valores e integridade

| Entrada | Comportamento |
|---|---|
| slug/title/body/kind/summary | Some obrigatório; apenas slug exige não vazio. NULL retorna RequiredField; os outros valores vazios são preservados. |
| body/title/kind | Texto UTF8 completo, BOM/CRLF/LF/espaços e valores editoriais custom são preservados; nenhum trim, fallback ou truncamento. |
| description | None, vazio e whitespace permanecem distintos. |
| summary/summary_truncated | Valores fornecidos preservados; não recalculados a partir do body. |
| source_path/observation.path | Não vazios; a projeção não prova captura nem autorização desses paths. |
| PhysicalSourceDigest32 | Digest físico opcional fornecido, sem hash interno. |
| CompilationDigest32 | Valor de compilação obrigatório separado do físico, sem conversão entre os tipos. |
| RecipeFields | id authored-document-v1 e parameters presentes; recipe desconhecida/incompleta retorna InvalidRecipe. |
| AuthoredParameters | fallback_title/kind_on_absent/max_summary_bytes positivo são preservados, sem reaplicá-los. |
| stale_after | Zero ou presente sem updated_at retorna InvalidPolicy. Comparações temporais pertencem ao kernel consumidor. |
| targets | Raw order, duplicados, self, vazio e dangling são preservados; validação/normalização do grafo pertence ao kernel ReferenceGraph. |

Observações são `Present { compilation_digest, recipe }` ou `Removed` explícito.
Ausência de uma observação não é remoção. Os parâmetros observados podem diferir
dos publicados: a configuração também participa da compilação. Duplicate slug/path
permanece input para os erros do consumidor. Construir digests ou dados Removed não
confere autoridade Published/Current; a integração precisa usar produtores reais.

## Limites e erros

Cinco limites positivos tratam entradas excessivas: `max_documents`,
`max_observations`, `max_rows`, `max_field_bytes` e `max_text_bytes`. Igualdade é
permitida. Rows contam headers, cada target bruto e cada observação. Field bytes
medem cada campo UTF8; text bytes somam as ocorrências fornecidas, incluindo recipe,
parâmetros, paths e targets repetidos. None opcional não acrescenta bytes.

Não há métricas de recursos, source/output/peak/work, reserva de CPU/RAM ou promessa
de RSS. Vec e Box usam alocação Rust comum. Soma que excede usize retorna Overflow;
entrada acima dos máximos retorna LimitExceeded. Erros incluem RequiredField,
InvalidRecipe, InvalidPolicy, InvalidPath, DifferentRequest e Request. Request
conserva io::Error e sua causa. Display não inclui conteúdo do documento. Um erro
interrompe a preparação; nenhuma coleção parcial é retornada.

Cancelamento é verificado na entrada, nos loops, ao concluir e nos getters.
Um request já cancelado vence erros de validação de dados. Deadline expirado conserva
TimedOut. A projeção não cria outro contexto para prolongar a execução.

## Validação e integração

Testes públicos verificam valores/pointers, campos ausentes, recipe, targets raw e
configuração diferente, os cinco limites de entrada, cancelamento/deadline/contexto
foreign, input vazio e grupos que sobrevivem aos descritores temporários e à
projeção, mantendo os dados originais vivos. Doctests compilefail cobrem os tipos
de digest, o slice de rows e a retenção do contexto.

Executar `cargo test -p graphit-data --test knowledge_document_projection` e
`cargo test -p graphit-data --doc knowledge_document_projection`, com fmt/Clippy
nos targets afetados. Guard global e integração final pertencem ao principal;
esta folha valida apenas os targets afetados.

As fronteiras Knowledge/Data/Records continuam
válidas quanto a Published/Current e fontes autorizadas. Esta API pura não implementa
storage, catálogo publicado, lifecycle ou aplicação de flags.
