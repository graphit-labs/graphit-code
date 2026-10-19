---
type: architecture
title: Avaliação documental Knowledge com referências e staleness em Rust
description: Composição de rows documentais com kernels reais e clock explícito.
references:
  - target: {type: task, id: tsk-ab72}
    relation: implements
  - target: {type: task, id: tsk-95fe}
    relation: implements
  - target: {type: task, id: tsk-0225}
    relation: uses
  - target: {type: task, id: tsk-55e8}
    relation: uses
  - target: {type: task, id: tsk-9c6e}
    relation: uses
  - target: {type: task, id: tsk-cfa56}
    relation: derived_from
  - target: {type: task, id: tsk-c9e2}
    relation: refined_by
---

# Avaliação documental Knowledge em Rust

A API `graphit_knowledge::staleness_projection` permite ao integrador usar uma
projeção documental Data para construir referências e avaliar staleness. A API
pura está integrada ao Knowledge principal; ela não captura fontes, publica
páginas nem aplica flags em storage. O integrador precisa obter dados publicados
e observações atuais completos e autorizados pelos produtores próprios.

## Fluxo e API

1. Preparar `PreparedKnowledgeDocumentRows` com a API
   [de projeção documental](rust-knowledge-document-projection.md), conservando
   textos, targets e o `SnapshotContext` original.
2. Fornecer `evaluated_at: SystemTime` e
   `StalenessProjectionLimits { references: ReferenceLimits, staleness: StalenessLimits }`.
   Não há clock interno ou limites implícitos.
3. Chamar `assess_projection(context, &projection, evaluated_at, limits)`.
4. Usar `assessment.graph(context)` e `assessment.stale_pages(context)` com o
   mesmo request. O grafo oferece as APIs existentes de vizinhos, links quebrados
   e estatísticas; o report contém apenas as páginas stale, ordenadas por slug.
5. O integrador decide como aplicar uma avaliação válida. Um erro não entrega
   avaliação parcial e precisa ser resolvido antes de qualquer alteração de flags.

O resultado `PreparedKnowledgeAssessment<'projection,'data,'context>` conserva
uma referência à projeção, o grafo e o report. Não pode sobreviver à projeção.
Getters emprestam o assessment e verificam o request original. O grafo conserva
o seu próprio contrato de contexto quando suas operações são usadas.

Arrays temporários de descritores podem ser descartados depois de preparar Data;
os owners do texto e do grupo original de targets precisam continuar vivos.
C não cria um Vec temporário de targets que escape, copia texto ou constrói um
owner self-referential. Os três Vecs locais de descritores dos kernels são comuns
e descartados por RAII após a avaliação.

## Mapeamento e integridade

| Dados Data | Consumidor |
|---|---|
| Header slug, title, grupo original de targets | `PageReferences` para `ReferenceGraph::build` |
| source_path e CompilationDigest32 | `SourceFingerprint` com digest copiado por valor |
| updated_at, stale_after, stale_since | `PublishedPage` literalmente |
| Observação Present com compilation digest | `SourceObservation/SourceState::Present` |
| Observação Removed explícita | `SourceObservation/SourceState::Removed` |

Headers imutáveis já contêm os mesmos targets raw das rows WikiTarget de H.
C usa esse grupo inteiro; as rows redundantes não exigem outro resolvedor.
Duplicados, self, vazio e dangling chegam ao kernel existente, que normaliza
links e valida identifiers. Observação ausente permanece ausente; não vira Removed.

O digest de compilação participa de staleness; o digest físico não participa.
Parâmetros publicados e observados podem diferir: se a compilação mudou, o kernel
retorna SourceChanged, mesmo com digest físico igual. Parâmetros diferentes com
digest de compilação igual não fazem C inventar mudança. Provar a correspondência
entre digest, recipe e fontes pertence aos produtores e à integração.

## Clock e razões

SourceChanged e SourceRemoved prevalecem sobre Age. Dependências são propagadas
pelas referências inbound; mudar uma página não torna seus destinos stale.
Razões diretas prevalecem sobre Dependency; múltiplas raízes seguem o desempate
determinístico do kernel. Paths e slugs retornados permanecem emprestados.

Com `updated_at=100`, `stale_after=50` e clock `200`, Age começa em `150`.
Com atualização `150`, o mesmo limite produz Age no boundary `200`.
Uma causa atual conserva `stale_since` anterior válido; o campo sozinho não
produz staleness. Timestamps futuros são inválidos mesmo quando existe
SourceChanged. A lista fresh é vazia: ela não carrega flags antigas nem as limpa
por conta própria.

A fixture com a→b, a→missing, b→c e d→e torna somente a stale quando sua fonte
muda. Com c removida explicitamente, o report ordenado contém a/b Dependency(c)
e c SourceRemoved. Sem observação de c, a operação retorna UnobservedSource.

## Limites, cancelamento e recuperação

C recebe diretamente os limites atuais dos kernels, sem contabilidade própria:

- ReferenceLimits: somente max_identifier_bytes, para identificadores individuais.
  O grafo permanece completo; não há quotas de páginas, edges raw ou texto agregado.
  Consultas de listas recebem max_results explicitamente por chamada.
- StalenessLimits: max_pages, max_graph_edges, max_observations, max_path_bytes,
  max_metadata_bytes e max_results.

Igualdade é permitida. Um excesso retorna o erro específico do kernel; ajustar
os limites exige avaliar o corpus esperado, sem truncar silenciosamente o resultado.
Os cinco limites de entrada Data continuam sendo definidos na preparação H.

`StalenessProjectionError` contém Projection, References ou Staleness e preserva
a cadeia de causas. DuplicatePage, DuplicateObservation, InvalidIdentifier,
InvalidPath, UnobservedSource e InvalidTimestamp chegam das APIs reais.
CorpusMismatch não é fabricado: grafo e PublishedPage vêm dos mesmos headers.
Corrigir inputs ou obter observações completas é responsabilidade do chamador.

O contexto original é checado antes de adaptar, nos loops, entre os kernels,
antes do retorno e nos getters. Cancelamento retorna a causa Interrupted;
deadline expirado retorna TimedOut. Contexto diferente retorna DifferentRequest.
Nenhum getter renova o request ou cria autorização de acesso às fontes.
Após cancelamento/timeout, o chamador pode abandonar o resultado; Rust descarta
as coleções normalmente.

## Verificação e integração

Os resultados e receipts anteriores são históricos e permanecem intactos.
A mudança de limites em `tsk-c9e2` foi qualificada em Linux: 157 funções
da biblioteca e dos 12 targets afetados, Clippy estrito nos mesmos targets,
compilação das duas fixtures App e três doctests modificados passaram.
O recibo `.build/rust-knowledge-reference-limits-cleanup/validation-current.json`
separa as falhas iniciais de compilação, o replay de referências e a evidência
atual. Windows e macOS não foram executados.

`cargo test -p graphit-knowledge --test staleness_projection` exercita H, grafo
e assess reais, incluindo as fixtures de mudança/remoção/configuração/idade,
erros de dados, todos os limites atuais e cancelamento/timeout/foreign.
`cargo test -p graphit-knowledge --doc staleness_projection` verifica o borrowing
do owner H. Fmt e Clippy são aplicados aos targets afetados.

O módulo público e a dependência Knowledge→Data estão no código principal.
Em Linux, seus nove testes funcionais, o doctest de borrowing, Clippy e
formatação passaram. Windows/macOS não foram executados nesta unidade.
A API não substitui os produtores Published/Current ou o lifecycle do dataset.
