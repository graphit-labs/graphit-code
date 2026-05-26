---
title: wikilinks
type: document
updated: 2026-05-26
tags: [knowledge, document, wiki]
---

# Links Bidirecionais (Wikilinks)

O **Graphit Code** utiliza o padrão de **links bidirecionais (wikilinks)** para criar conexões entre documentos técnicos na wiki de conhecimento. Esse padrão é popularizado por ferramentas como Obsidian.

---

## 🔗 Sintaxe de Link

Para linkar de uma página para outra na wiki, utilize colchetes duplos envolvendo o slug/basename da página de destino:

```markdown
Consulte o [[setup_guide]] para instruções de instalação.
```

Quando a wiki é indexada pelo comando `graphit knowledge index`, o sistema resolve `[[setup_guide]]` buscando uma página que possua a propriedade `title: setup_guide` ou que tenha o nome de arquivo `setup_guide.md`.

---

## 📈 Grafo de Referências Cruzadas e Backlinks

O motor de conhecimento do Graphit Code analisa todas as referências `[ [slug] ]` nos arquivos Markdown e gera um **Grafo de Referências Cruzadas**. 

Com base neste grafo, o sistema realiza as seguintes ações automaticamente:
1. **Injeção de Backlinks:** No final de cada página gerada, insere-se uma seção `## Backlinks` listando todas as páginas da wiki que referenciam o documento atual.
2. **Navegação Bidirecional:** Permite que a IA e o desenvolvedor naveguem tanto para a frente (links diretos) quanto para trás (backlinks).
3. **Busca Progressiva:** A IA utiliza estas referências para decidir quais outros documentos ler durante a busca contextual do chat.

---
*Este documento detalha o sistema de wikilinks interno do Graphit Code.*
