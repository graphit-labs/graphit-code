---
title: Leitura autenticada de fonte Knowledge no App
doc_type: architecture
references:
  - target: {type: task, id: tsk-3032}
    relation: implements
  - target: {type: task, id: tsk-21cd}
    relation: uses
  - target: {type: task, id: tsk-322f}
    relation: implements
  - target: {type: task, id: tsk-5de4}
    relation: uses
  - target: {type: task, id: tsk-72ce}
    relation: uses
  - target: {type: task, id: tsk-8218}
    relation: uses
  - target: {type: task, id: tsk-7d1e}
    relation: uses
  - target: {type: task, id: tsk-1f4a}
    relation: uses
---

# Leitura autenticada de fonte Knowledge no App

A leitura de [Markdown](rust-knowledge-document-source.md) usa a autorização
Knowledge do contexto selecionado. O slug identifica a página publicada. No
PROJECT, a seleção resolve o checkout e a versão retida. No importado, o ID e
a versão identificam a instalação, com alias opcional. A autorização de
artefato não depende de um PROJECT ativo.

MCP e Web usam o mesmo leitor de páginas publicadas. O leitor de armazenamento
seleciona pelo slug no corpus admitido e mantém a verificação de acesso durante
a leitura. A CLI usa a operação MCP. No leitor web, os destinos de navegação
são resolvidos na mesma publicação. A rota de referências tem admissão própria.

A operação distingue corpus ausente, corpus vazio, página ausente e página
encontrada. Uma seleção importada também pode identificar um artefato ausente.
O armazenamento publicado, os arquivos Markdown e os IDs seguem legíveis sem
migração para este contrato de leitura.
