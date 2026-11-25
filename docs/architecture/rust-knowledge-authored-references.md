---
type: architecture
title: Referências authored de documentos Knowledge em Rust
description: Adapter puro e limitado entre metadata documental admitida e referências lógicas Records.
references:
  - target: {type: task, id: tsk-9983}
    relation: implements
  - target: {type: task, id: tsk-bd00}
    relation: uses
  - target: {type: task, id: tsk-5530}
    relation: uses
  - target: {type: task, id: tsk-41d1}
    relation: relates_to
  - target: {type: task, id: tsk-c9e2}
    relation: refined_by
---

# Referências authored de documentos Knowledge em Rust

O adapter transforma o campo `references` de um documento admitido pelo parser
Knowledge em uma decisão completa de preservar ou substituir referências
authored. Reutiliza o kernel lógico Records, sem consultar destinos, conceder
acesso, publicar projeções ou alterar o documento.

## API e ownership

A API pertence a `graphit_knowledge::document_authored_references`:

```rust
pub struct DocumentAuthoredReferenceLimits {
    pub max_results: NonZeroUsize,
    pub max_identifier_bytes: NonZeroUsize,
}

pub fn project_document_authored_references<'text>(
    original: &SnapshotContext,
    parsed: &'text ParsedDocument<'_>,
    source: SourceNamespace<'text>,
    parse_limits: DocumentParseLimits,
    limits: DocumentAuthoredReferenceLimits,
) -> Result<AuthoredReferenceUpdate<'text>, DocumentAuthoredReferenceError>;
```

O caller fornece um `ParsedDocument` do
[parser documental](rust-knowledge-document-parser.md), seu namespace lógico,
os limites explícitos de parsing e os limites existentes de referências.
`graphit-knowledge` depende do produtor `graphit-records`, que define namespace,
candidatos e resultado. Não há dependência Data → Records.

O resultado empresta textos do owner ParsedDocument e do namespace. Scalars
que o parser decodificou em alocações owned continuam pertencendo a esse owner.
O resultado não empresta a lista temporária de candidatos, as strings
semânticas temporárias ou o contexto da operação. Mantenha os owners de texto
vivos durante o uso do resultado.

ParsedDocument não retém identidade de request nem a política anterior.
O adapter verifica a liveness do único Original fornecido; não prova que esse
contexto criou o ParsedDocument. A vinculação de captura e request de um futuro
caller CompiledDocument pertence ao seu produtor.

## Admissão e preserve/clear

O parser existente admite ausência de frontmatter e frontmatter vazio.
Conteúdo YAML exige uma raiz mapping. Raiz explicitamente null, raiz não
mapping, chaves null/complexas e chaves literais duplicadas são rejeitadas pelo
produtor antes da admissão do documento.

| Documento admitido | Resultado |
| --- | --- |
| Sem frontmatter, frontmatter vazio ou mapping sem `references` | `Preserve` |
| `references: []` | `Replace` com vetor vazio |
| Sequência válida | `Replace` com todas as referências, em ordem |
| Campo null, implicitamente null ou não sequência | Erro |
| Entrada inválida, mesmo após entradas válidas | Erro da operação inteira |

Preserve não é clear. O caller aplica a decisão às referências authored;
links estruturais e links Wiki possuem seus próprios produtores.

Cada entrada usa um mapping estrito:

```yaml
---
title: Exemplo
references:
  - target:
      type: memory
      id: "001"
    relation: "supports"
    field: ""
---
# Corpo preservado
```

Esse ID é literal; o exemplo não verifica a existência de uma Memory.
`target.type`, `target.id` e `relation` são obrigatórios. `target.scope`,
`target.scope_id`, `target.context` e `field` são opcionais. Campos
aninhados desconhecidos são inválidos. Omissão de um opcional é None;
null explícito não é omissão.

Os quatro tipos são exatamente `task`, `session`, `memory` e `knowledge`.
IDs e relações são strings não vazias, preservadas sem trim ou normalização.
`field: ""` é permitido. Ordem e duplicatas de entradas permanecem intactas.

## Semântica YAML e integridade

MetadataKind e ScalarStyle descrevem sintaxe; não provam que um scalar Plain
é uma string segundo o schema YAML. Antes de devolver referências borrowed,
o adapter verifica bytes reais, reexecuta `parse_document` sobre a fonte real
com os limites fornecidos e compara envelope, raiz e todos os nodes, incluindo
spans, estilo, shape e texto, com o ParsedDocument recebido.

Estatísticas públicas e título projetado não são autoridade. Inconsistência
produz `InvalidEnvelope`, sem uma decisão falsa de preservar referências.
A sequência e seus mappings são validados antes de delegar ao kernel Records.

Somente scalars reservados Plain recebem probe independente sobre seu raw
span, usando serde-saphyr 1.3 e um visitor `deserialize_any` que aceita strings.
Com `no_schema: false`, `strict_booleans: false` e
`reject_non_finite_typeless_float: true`, booleanos YAML 1.1 como `yes`/`on`,
números, null, `1e999`, `.nan` e `.inf` não são convertidos em strings.
As formas citadas, como `"true"` e `"001"`, conservam o texto admitido.

Scalars quoted, literal e folded são admitidos no frontmatter completo,
no contexto original. Não são decodificados isoladamente ou dedentados:
isso preserva `|2`, `>2` e strings citadas multilinha.

O decode do frontmatter inteiro usa FirstWins e tolerância nonfinite para
preservar metadata não relacionada, com valores desconhecidos consumidos por
IgnoredAny. O probe estrito anterior impede que essa tolerância admita campos
reservados Plain não textuais. A comparação final confere cada tupla e string
com o owner AST antes da projeção lógica.

Chaves numéricas literais distintas, como `0xB` e `11`, e valores desconhecidos
como `title: .nan` ou sequências com `.inf`/`1e999` continuam admitidos.
Isso não admite chaves null, raiz null ou duplicatas literais já rejeitadas
pelo parser.

## Namespace lógico

As regras vêm do [kernel Records](../specs/record_relations.md):

- PROJECT usa o ProjectId Core canônico; USER usa um subject literal não vazio.
- Scope omitido herda o tipo do namespace fonte.
- Scope ID omitido herda somente quando o tipo escolhido coincide com o da fonte.
- Context de Knowledge omitido herda somente quando o scope resolvido coincide
  integralmente com o da fonte, incluindo tipo e ID.
- Context explícito é exclusivo de Knowledge e deve ser não vazio.

Identidades estrangeiras ou unresolved permanecem dados lógicos. O adapter
não testa existência, permissão ou disponibilidade dos destinos.

## Limites e erros

Todos os caps são explícitos. Igualdade com o cap é admitida; overflow e excesso
são erros. Os bytes reais de source respeitam `max_document_bytes` e
`max_input_bytes`. O raw span YAML respeita `max_frontmatter_bytes` antes do
decoder; a revalidação do parser também conta os delimitadores de frontmatter
segundo sua política existente e aplica os limites reais de eventos,
profundidade, scalars e projeção, sem confiar em stats.

`DocumentAuthoredReferenceLimits.max_results` limita a saída 1:1 de tuplas
por documento antes da reserva do vetor. Igualdade é admitida; uma tupla além
do limite retorna `LimitExceeded("reference results")` sem resultado parcial.
Ordem e duplicatas permanecem literais. `Preserve` e `Replace` vazio conservam
suas semânticas.

`max_identifier_bytes` limita individualmente id, scope_id, context e os textos
borrowed USER/context do namespace fonte. Não há soma textual de candidatos ou
namespace, limite de corpus ou ledger de referências. FILE, frontmatter, arena,
eventos e scalars continuam sujeitos aos limites reais de `DocumentParseLimits`.
Esse tipo é separado de `ReferenceLimits`, usado pelo grafo.

O SDK recebe budgets derivados de DocumentParseLimits: eventos/nodes,
profundidade/flow, scalars e lookahead limitado pelo frontmatter. Usa um único
documento, sem aliases, anchors, merge, inclusão ou expansão de propriedades.
Comentários não são retidos. O input já é um str limitado;
`max_reader_input_bytes` não substitui essa admissão.

`DocumentAuthoredReferenceError` distingue Request, Syntax, SemanticYaml,
Kernel, InvalidEnvelope, LimitExceeded e Allocation.
`Error::source()` conserva causas reais de request, parser, SDK, kernel e
TryReserveError. Falhas locais em callbacks semânticos preservam a primeira
causa tipada, sem classificação pelo texto do erro SDK.

O Original é verificado nas fronteiras controladas, entre entradas e pelo
kernel antes do sucesso. Uma fase síncrona interna do SDK não é preemptível.
Não há promessa de latência máxima ou de recuperação de toda OOM nativa.

## Verificação e integração futura

Os resultados e receipts anteriores são históricos e permanecem intactos.
A mudança de limites em `tsk-c9e2` foi qualificada em Linux: 157 funções
da biblioteca e dos 12 targets afetados, Clippy estrito nos mesmos targets,
compilação das duas fixtures App e três doctests modificados passaram.
O recibo `.build/rust-knowledge-reference-limits-cleanup/validation-current.json`
separa as falhas iniciais de compilação, o replay de referências e a evidência
atual. Windows e macOS não foram executados.

As fixtures usam ParsedDocument reais: preserve/clear/null, quatro kinds,
namespaces, ordem/duplicatas, strings citadas versus scalars tipados, metadata
desconhecida, `|2`/`>2`/multilinha/CRLF, integridade de spans e estilos, caps,
cancelamento, deadline e lifetime borrowed.

```text
cargo test --offline --locked -p graphit-knowledge --test document_authored_references -j1
cargo test --offline --locked -p graphit-knowledge --doc document_authored_references -j1
cargo clippy --offline --locked -p graphit-knowledge --lib --test document_authored_references -j1 --no-deps -- -D warnings
```

O `receipt` `.build/rust-knowledge-authored-references/validation.json` registra
os comandos, resultados e falhas históricas. A qualificação histórica desta unidade se
restringe à API pura em Linux e ao doctest de lifetime. Não qualifica outras
plataformas ou uma jornada Browser/IAB.

Persistência/projeção corrente, query, lifecycle, importação e resolução rica
autorizada permanecem entregas próprias de 41d1 e seus produtores. O resultado
puro não é prova de publicação, grant ou FullNoLoss.
