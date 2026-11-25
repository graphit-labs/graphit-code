---
type: specification
title: Domínios e comunidades documentais Knowledge em Rust
description: Classificação topológica do grafo completo com report owned e integração de exportação separada.
references:
  - target: {type: task, id: tsk-d544}
    relation: implements
  - target: {type: task, id: tsk-6297}
    relation: derived_from
  - target: {type: task, id: tsk-11595}
    relation: uses
  - target: {type: task, id: tsk-a99d}
    relation: consumed_by
  - target: {type: task, id: tsk-c9e2}
    relation: uses
---

# Domínios e comunidades documentais Knowledge em Rust

O kernel organiza documentos existentes em comunidades topológicas calculadas
na leitura. Kind e tags editoriais permanecem independentes. Não gera páginas,
inferência AI, campos persistidos ou novas identidades; body, referências,
fingerprints, recipe e embeddings permanecem intactos.

## API e ownership

```rust
pub struct DocumentCommunity {
    pub members: Vec<String>,
    pub label: String,
    pub cohesion: f64,
}
pub struct DocumentCommunities {
    pub groups: Vec<DocumentCommunity>,
}
pub fn classify_document_communities(
    original: &SnapshotContext,
    graph: &ReferenceGraph<'_, '_>,
) -> Result<DocumentCommunities, DocumentCommunityError>;
```

As structs derivam Debug e PartialEq. O caller fornece o
[ReferenceGraph existente](../architecture/rust-knowledge-wiki.md).
Accessors crate-private vinculados ao mesmo Original emprestam títulos e
outbound canônicos para leitura. Não há segundo ReferenceGraph ou repetição
da admissão de IDs e corpus.

O report é owned e permanece utilizável depois de liberar os owners do grafo
e de seus textos. Não retém request, pin, grant, pathname ou geração.
O caller associa o report à geração observada daquele corpus; o kernel
não prova uma nova fronteira de autorização ou escopo.

Não há ID persistente de comunidade ou assignments adicionais no resultado.
O primeiro slug pode representar o grupo somente naquela observação.
Documento omitido dos grupos significa ausência de comunidade.

## Adaptação e resultados

Todos os documentos conhecidos viram nodes SDK com índices internos em ordem
lexical de slug. Ligações conhecidas viram pares undirected `(min, max)`
canônicos e únicos. Self-links, duplicatas e reciprocidade não acrescentam
peso; destinos ausentes não viram nodes. O grafo original conserva estatísticas
funcionais, referências direcionais e diagnósticos de dangling.

A resposta SDK precisa ser uma partição completa e disjunta dos IDs admitidos
antes de omitir singletons. Membro desconhecido, repetido ou ausente produz
InvalidPartition, sem report parcial. Uma lookup compacta privada serve à
verificação e à contagem funcional dos pares internos; não é quota ou segundo
report de assignments.

Os membros são slugs exatos em ordem lexical. Grupos usam tamanho decrescente
e desempate pelo vetor lexical de membros. Case e Unicode são preservados,
sem trim, normalização ou merge. Grupos de tamanho um são omitidos: vazio,
documento único e corpus sem ligações conhecidas retornam `groups: []`.

O label é o título do membro com maior grau total no grafo simples conhecido,
incluindo vizinhos fora do grupo. Empate escolhe o menor slug. Somente título
literalmente vazio usa o slug como fallback.

Para `n >= 2`, coesão é a quantidade de pares internos distintos dividida por
`n * (n - 1) / 2`, arredondada a duas casas decimais. O denominador usa ponto
flutuante sem multiplicação usize intermediária; a densidade é finita em
`[0, 1]`. Não é o score de modularidade do SDK.

| Fixture | Resultado esperado |
| --- | --- |
| `a → b` | Membros `[a,b]`, label de a no empate, coesão 1.0 |
| Duas cliques disjuntas de três membros | Dois grupos lexicais empatados, coesão 1.0 |
| Cadeia `a–b–c` | Grupo de três, label de b, coesão 0.67 |
| Singletons e destino somente ausente | Nenhum grupo publicado |
| Duplicate/self/reciprocal/dangling no par | Mesmo report do par; grafo original intacto |

## Biblioteca, Original e erros

O produtor Root `tsk-11595` instalou `graphrs = 0.12.0` como dependência normal
de Knowledge, com `default-features = false`, sem adjacency_matrix/LAPACK.
O [manifest oficial](https://raw.githubusercontent.com/malcolmvr/graphrs/v0.12.0/Cargo.toml)
declara licença MIT e matriz opcional. A
[release 0.12.0](https://github.com/malcolmvr/graphrs/releases/tag/v0.12.0)
acrescenta PageRank.

O kernel chama `louvain_communities(&sdk_graph, false, None, None, Some(0))`.
A seed seleciona o caminho que ordena candidatos; não substitui os testes
Graphit de repetição e permutação. O
[Source Louvain](https://docs.rs/graphrs/0.12.0/src/graphrs/algorithms/community/louvain.rs.html)
ignora `_threshold` e não recebe callback de cancelamento. O refinamento 6297
comparou a extração textual docs.rs de 0.11.16 e 0.12.0, incluindo números de
linha, com SHA-256 idêntico:

```text
20f25788614a07a4b65c96fa187afebc94eb86bf8da82fc3f62aef207f4899c2
```

O Original é checado antes dos accessors, por node/edge e saída, antes e depois
do SDK e antes do sucesso. Request equivalente por deadline não substitui a
identidade real do grafo. Cancelamento ou deadline observado após o SDK descarta
o report inteiro. SDK e ordenação síncronos não prometem preempção ou latência
máxima.

DocumentCommunityError distingue References(ReferenceError), Request(io::Error),
Library(graphrs::Error), Allocation(TryReserveError) e InvalidPartition.
Error::source conserva References, Request e Allocation como causas StdError,
sem classificação por mensagem. `graphrs::Error` não implementa StdError:
Library conserva o erro SDK tipado e seu Display, mas source() retorna None.
Não há wrapper criado apenas para forçar essa trait.
Encaminhar Library não demonstra uma falha SDK alcançável com a configuração
normal; a qualificação não fabrica erro de biblioteca.

Reservas e cópias owned controladas pelo kernel são fallíveis. Não há limite
de RSS, recuperação garantida de toda OOM SDK, DocumentCommunityOutputLimits,
quota nova de grupos/membros/label ou truncamento. Identificadores individuais
do grafo e controles reais de parser/transporte permanecem em seus produtores.

## Consumidor de exportação separado

A integração `tsk-a99d` está implementada na
[exportação Markdown](../architecture/rust-knowledge-markdown-export.md).
As suítes focais Knowledge e App passaram em Linux; a evidência própria não
altera o receipt histórico de qualificação do kernel.

A classificação ocorre após o único ReferenceGraph, antes da preparação do
payload e do writer filesystem. Usa as mesmas rows FULL EOF e geração
Knowledge observada usadas na exportação. MarkdownExportPlan contém report
owned em communities. App separa payload/report do plano e encerra o empréstimo
da geração antes de movê-la com as comunidades para KnowledgeMarkdownPublished,
inclusive ArtifactChange::Unchanged. Nenhum field de communities é adicionado
a MarkdownExportLimits.

Não há segundo read, pin ou grafo. Data libera scanner, pin e store antes de
retornar as rows; o resultado não promete pin SDK viva na publicação filesystem
ou head ainda atual. Report não é serializado no payload: bytes, 15 campos
de metadata, filenames, navegação e fingerprint permanecem intactos.

MarkdownExportError::Communities conserva o erro do kernel antes do writer.
Sua cadeia de causas alcança DocumentCommunityError; Library mantém o SDK
tipado e Display, sem causa StdError, conforme o protocolo real de graphrs.

NoCorpus e PublishedEmpty mantêm variantes sem report. Corpus somente de
documentos isolados produz sucesso com report vazio. Recovery não possui rows
e conserva API. Erros e efeitos posteriores à preparação/publicação continuam
causais; não há report inventado ou rollback.

Sete funções export Knowledge, sete regressões kernel e 16 funções App passaram,
incluindo três funções novas desta integração. A geração ordinary diferente
com payload idêntico é prova pura Knowledge. App prova republicação Data real,
novo label/geração com Updated e depois Unchanged naquela geração. Não se
atribui a esse caso uma nova geração SDK com mesmos bytes filesystem.
O receipt `.build/rust-knowledge-community-export/validation-current.json`
conserva essa distinção; Clippy estrito nos targets focais passou; a qualificação documental final
consta no receipt da integração. Windows/macOS não foram executados.

## Qualificação do kernel em Linux

Passaram sete funções públicas do kernel, um caso privado pós-SDK e os
13 testes existentes de referências. O replay final das sete funções após
acrescentar a ponte entre cliques não aumenta essa contagem. A ponte protege
label por grau total mesmo quando há vizinho fora do grupo. Clippy estrito
passou nos targets afetados; partições e repetição/permutação foram executadas
contra o SDK real. Esses casos não prometem comportamento entre versões SDK.

O recibo `.build/rust-knowledge-document-communities/validation-current.json`
registra comandos, contagens e a primeira falha de compilação E0277, corrigida
sem wrapper de erro. Windows e macOS não foram executados. A matriz protege
partições, labels, coesão, Unicode/case, permutação/repetição, normalização,
ownership, limites de identificador existentes e causas Original.

O caso privado pós-SDK cancela o mesmo Original depois de obter uma partição
real, antes da conversão final; não substitui o algoritmo por stub.

```text
cargo test --offline --locked -p graphit-knowledge --test document_communities --test references -j4
cargo test --offline --locked -p graphit-knowledge --lib document_communities -j4
cargo clippy --offline --locked -p graphit-knowledge --lib --test document_communities --test references -j4 --no-deps -- -D warnings
```

Kernel, integração App, plataformas e validação ponta a ponta têm provas distintas.
Esta página não declara a entrega App ou a validação ponta a ponta concluída.
