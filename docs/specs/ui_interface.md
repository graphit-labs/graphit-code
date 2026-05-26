---
title: ui_interface
type: specification
updated: 2026-05-26
tags: [knowledge, specification, ui]
---

# Interface do Usuário (Web UI Dashboard)

O Graphit Code oferece um painel administrativo e explorador web completo (`internal/ui`). Ele é construído como uma Single Page Application (SPA) moderna usando React, TypeScript, TailwindCSS e Zustand para gerenciamento de estado.

---

## 🎨 Design System e Layout Core

O painel foi estruturado seguindo tendências visuais de alta fidelidade e ergonomia de desenvolvedor:
* **ThemeProvider:** Suporta alternância dinâmica de temas (Dark Mode / Light Mode) e persistência de preferências de cor HSL no armazenamento do navegador.
* **AppShell & Sidebar:** Navegação responsiva de duas camadas, fornecendo atalhos rápidos para alternar contextos do sistema (Hub, AST, Wiki e Memória) e um seletor rápido de projetos ativos (`ProjectSwitcher`).
* **Zustand AppStore:** Centraliza as chamadas de API locais, listagem de projetos ativos, estados globais de carregamento (`GlobalLoader`) e toasts de notificações de sucesso/erro.

---

## 🌳 Explorador de AST (`/ast`)

O explorador AST é a tela de engenharia reversa de código fonte do projeto. Ele divide-se em duas visões:

### 1. Contextos da AST (`/ast/contexts`)
Lista todos os bancos de dados de grafos AST instalados localmente. Permite visualizar quais projetos estão indexados na máquina e importar novos diretórios de código fonte criando contextos isolados.

### 2. Painel Explorer (`/ast/explorer/:contextId`)
Uma tela avançada composta por seis áreas de trabalho dinâmicas integradas:

```
+-------------------------------------------------------------+
| QueryBar: Campo para Cypher ou IA Natural Language          |
+------------------------------+------------------------------+
| Explorer Tree (Esquerda):    | Canvas 3D (Centro):          |
| Estrutura de arquivos e nós  | Grafo interativo 3D          |
|                              |                              |
+------------------------------+------------------------------+
| CodePanel & SchemaPanel:     | TabularResults:              |
| Código fonte e propriedades  | Resultados da query em lista |
+------------------------------+------------------------------+
```

* **QueryBar:** Entrada de busca de código. Permite digitar consultas Cypher brutas ou escrever perguntas em linguagem natural (utilizando o Query Cipher para gerar o código Cypher correspondente).
* **NodeTree:** Uma árvore colapsável mostrando a estrutura física de arquivos, módulos, classes e funções indexados no repositório.
* **SchemaPanel:** Mostra os metadados do esquema do grafo (rótulos de nós e tipos de arestas de relacionamento).
* **CodePanel:** Editor de código integrado de somente leitura. Quando um nó correspondente a uma função, variável ou arquivo é clicado, o painel abre automaticamente o arquivo fonte posicionado exatamente no número da linha de definição do elemento.
* **TabularResults:** Exibição em lista clássica dos nós retornados em consultas personalizadas.
* **GraphCanvas (Renderizador 3D):** Visualizador de grafos interativo tridimensional alimentado pela biblioteca **d3-force-3d**. Ele renderiza nós e arestas no espaço 3D usando física de força física direcionada. Os desenvolvedores conseguem arrastar, girar, dar zoom e clicar em nós de funções ou classes para visualizar as cadeias de chamadas (`CALLS`) e imports de forma espacial.

---

## 📚 Central do Wiki Chat (`/wiki`)

A interface da Wiki permite pesquisar e interagir sobre documentações técnicas:
* **WikiSearchPage:** Barra de pesquisa minimalista simulando motores de busca globais. O desenvolvedor digita a pergunta e escolhe as fontes de documentação (Wiki, Memória ou Projetos irmãos).
* **WikiSearchResultsPage:** Exibe a resposta final gerada pela IA após o ciclo de busca progressiva.
  * O histórico de chat permite realizar perguntas complementares em uma interface de chat fluida.
  * Links no formato `[ [Page_Name] ]` geram links navegáveis que, ao serem clicados, abrem a página correspondente no leitor integrado da wiki (`WikiExplorerPage`).

---

## 🤝 Painel do Hub de Artefatos (`/hub`)

Facilita a colaboração e publicação de recursos:
* **RegistryPage:** Catálogo de artefatos online disponíveis no repositório compartilhado. Permite buscar, filtrar e instalar novas regras de IA, skills de agentes ou documentações contextuais com um clique.
* **ProjectArtifactsPage:** Lista todos os artefatos instalados e vinculados no projeto local atual, facilitando desinstalações e atualizações rápidas de versão.
* **UploadPage:** Formulário para publicação de novas ferramentas locais. O desenvolvedor seleciona o arquivo ou diretório de skill, preenche metadados (versão, nome, descrição, tags) e realiza o submit direto para o Hub.

---
*Próximo passo recomendado:* Veja o arquivo de logs e histórico da [[index]].
