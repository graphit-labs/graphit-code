---
type: architecture
title: Persistência nativa de referências Knowledge em Rust
description: Estado documental, referências autorais e atualização de publicações anteriores.
references:
  - target: {type: task, id: tsk-c8e9}
    relation: implements
  - target: {type: task, id: tsk-41d1}
    relation: relates_to
  - target: {type: task, id: tsk-72ce}
    relation: uses
  - target: {type: task, id: tsk-c1a4}
    relation: uses
  - target: {type: task, id: tsk-8b24}
    relation: relates_to
  - target: {type: task, id: tsk-9983}
    relation: relates_to
  - target: {type: task, id: tsk-185c}
    relation: refined_by
---

# Persistência nativa de referências Knowledge em Rust

O Lance publica o documento, seu estado e suas referências na mesma transação.
Uma projeção ausente indica cobertura incompleta. Uma lista autoral vazia
indica que a origem declarou explicitamente zero relações. Os links wiki
continuam separados, na ordem do Markdown publicado.

## Referências e escopo

A projeção contém o escopo da origem, o contexto opcional e a lista de
referências autorais. Cada referência preserva tipo, ID, escopo, contexto,
relação e campo. IDs, ordem e duplicatas são dados do autor. O namespace
persiste com a página, inclusive quando o artefato é importado com um alias.

A atualização sem `references` mantém a lista anterior. `references: []`
limpa a lista autoral. Uma remoção aposenta a página e publica relações
vazias. Leitores de referências ignoram linhas removidas após validar o estado
e a estrutura da projeção.

## Publicações anteriores

Publicações antigas têm um campo extra, `reference_projection.witness`.
O leitor aceita esse schema e usa os mesmos campos de estado, escopo e
referências que nas publicações novas. O campo antigo é descartado da
projeção atual durante uma escrita autorizada. A reconciliação incremental
usa a alteração de schema do Lance sob o lock e o CAS já usados para escrita.
A atualização que regrava uma imagem publica diretamente o schema atual.

O descarte de metadados mantém fragmentos, vetores e versões retidas. Os
artefatos instalados e as versões históricas continuam legíveis. Markdown,
slugs e referências permanecem associados às mesmas linhas.

A API de leitura expõe a versão da publicação. Para continuar uma busca ou
consulta de referências, devolva o cursor com os mesmos seletores. O servidor
valida o vínculo com a publicação autorizada. Um cursor de outra publicação
requer reiniciar a leitura.

Veja [consulta de referências](../specs/rust_knowledge_reference_query.md) e
[republicação](../specs/rust_knowledge_document_republication.md).
