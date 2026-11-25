---
title: Identidade das fontes Knowledge em Rust
type: spec
references:
  - target: {type: task, id: tsk-cd410}
    relation: implements
  - target: {type: task, id: tsk-990e}
    relation: specified_by
  - target: {type: task, id: tsk-3f7e}
    relation: consumer_boundary
  - target: {type: task, id: tsk-c8e9}
    relation: preserves
---

# Identidade das fontes Knowledge em Rust

O Knowledge publica origens gerenciadas como caminhos relativos legíveis. Por
exemplo, `docs/ação.md` é armazenado como `knowledge-source/path/docs/ação.md`.
O prefixo identifica arquivos gerenciados pelo indexador. Outros produtores
continuam com suas origens literais. Unicode e caixa são preservados.

`KnowledgeSourceIdentity::from_relative_path` aceita caminhos relativos com
componentes normais e valida o ID autoral quando presente. Um ID autoral
sobrevive à mudança de caminho. Para fontes sem ID, o ULID continua derivado
da sequência de bytes usada nas publicações antigas. O mesmo arquivo, incluindo
links Markdown para ele, conserva seu slug depois da atualização.

## Publicações anteriores

O leitor de origens reconhece `knowledge-source/t/...` em publicações
anteriores e obtém os mesmos componentes relativos. A seleção de um arquivo
em uma atualização inclui a representação anterior e a legível. A
republicação grava a origem legível para os arquivos que processa, preserva
referências autorais omitidas e mantém documentos de outros produtores. A
remoção usa a mesma seleção e requer que a captura autorizada confirme a
ausência do arquivo. A publicação retém a verificação da geração para
proteger contra alterações concorrentes.

Componentes de origem são dados. A conversão em caminho da plataforma
valida cada componente antes de usar a captura autorizada para acessar o
arquivo. Uma contrabarra em um nome Unix permanece literal.

As [fixtures de identidade](../../crates/graphit-data/tests/knowledge_source_identity.rs)
comparam os ULIDs históricos, incluem Unicode e verificam a equivalência
entre as representações. O [teste de republicação](../../crates/graphit-data/src/knowledge_document_lifecycle/tests.rs)
abre a origem anterior no Lance, atualiza o caminho e verifica corpo, slug,
referências e linhas alheias.
