---
title: Leitura de Markdown no Knowledge
doc_type: architecture
references:
  - target: {type: task, id: tsk-5de4}
    relation: implemented_by
  - target: {type: task, id: tsk-b210}
    relation: specified_by
  - target: {type: task, id: tsk-72ce}
    relation: uses
  - target: {type: task, id: tsk-8218}
    relation: uses
  - target: {type: task, id: tsk-8b24}
    relation: integrates
  - target: {type: task, id: tsk-7d1e}
    relation: uses
  - target: {type: task, id: tsk-cab0}
    relation: implements
---

# Leitura de Markdown no Knowledge

Use `graphit_wiki_source` com o slug da página. A resposta `found`
traz `slug`, `title` e `selection`. Sem seletores, `selection.source`
contém o Markdown publicado completo, incluindo o frontmatter autoral quando
presente. A CLI `graphit knowledge source <slug>` escreve esse texto em stdout.
Com `--json`, entrega a resposta estruturada.

```json
{"slug":"alpha","selectors":{"start_line":1,"end_line":20,"line_numbers":true}}
```

`start_line`, `end_line` e `total_lines` usam as linhas do Markdown
original. Uma busca por padrão sem ocorrências devolve uma seleção vazia
para a página encontrada. Uma leitura acima do limite de resposta falha e
pode ser repetida com um recorte explícito.

O leitor web mostra o documento e resolve os vínculos anteriores, próximos e
relacionados dentro da publicação selecionada. A consulta de referências
recebe o slug e o namespace lógico da página.

- [Admissão PROJECT e de artefatos](rust-knowledge-source-admission.md)
- [Exportação Markdown](rust-knowledge-markdown-export.md)
