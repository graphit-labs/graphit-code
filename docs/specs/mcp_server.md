---
title: mcp_server
type: specification
updated: 2026-05-26
tags: [knowledge, specification, mcp]
---

# Especificação do Servidor MCP (Model Context Protocol)

O **Servidor MCP** (`internal/mcpserver`) expõe o ecossistema do Graphit Code para IAs agentes (como Claude Code, Cursor, VS Code Copilot e outras). Ele permite que os LLMs consultem o grafo AST, busquem na documentação wiki e acessem/salvem memórias diretamente através de uma API padronizada.

---

## 🚀 Modos de Transporte

O servidor MCP pode ser executado em dois modos de transporte de dados:

1. **Stdio Transport (`graphit mcp` ou `graphit mcp --stdio`):**
   Comunicação via canais de entrada e saída padrão (Stdin/Stdout) de processos. É o padrão utilizado por IDEs locais (como Cursor) e utilitários de CLI locais (como Claude Code) para subir o servidor como um processo filho seguro.
2. **HTTP Streamable Transport (`graphit mcp --port 8282`):**
   Comunicação via Server-Sent Events (SSE) sobre HTTP local (`http://127.0.0.1:8282/mcp`). Permite conexão de clientes externos ou múltiplos agentes compartilhando a mesma base local.

---

## 🛠️ Ferramentas Registradas (MCP Tools)

O servidor expõe as seguintes ferramentas agrupadas por módulo:

### 🌳 Grafo AST (`graphit_ast_`)
Ferramentas de acesso ao grafo de sintaxe abstrata do código fonte:

| Nome da Ferramenta | Descrição | Parâmetros Principais |
|---|---|---|
| `graphit_ast_query` | Executa uma consulta Cypher bruta no LadybugDB do projeto. Retorna JSON. | `query` (string), `context` (string) |
| `graphit_ast_search_fts` | Busca textual BM25 nas tabelas SQLite FTS5 indexadas do código. | `query` (string), `top_k` (int) |
| `graphit_ast_search_semantic` | Busca semântica vetorial KNN usando embeddings das entidades. | `query` (string), `top_k` (int) |
| `graphit_ast_query_ai` | Traduz pergunta do usuário em Cypher usando IA, executa-o e retorna os resultados. | `query` (string), `context` (string) |
| `graphit_ast_schema` | Retorna a estrutura (esquema de nós e relacionamentos) do banco de grafos. | `context` (string) |

### 📂 Registro Colaborativo (`graphit_hub_`)
Ferramentas de sincronização e download de artefatos compartilhados no Hub:

| Nome da Ferramenta | Descrição | Parâmetros Principais |
|---|---|---|
| `graphit_hub_list` | Lista os artefatos disponíveis para instalação na réplica local do Hub. | `type` (string) |
| `graphit_hub_search` | Busca artefatos no Hub correspondendo ao termo de busca textual. | `query` (string), `type` (string) |
| `graphit_hub_show` | Retorna metadados e arquivos de um artefato específico do Hub. | `artifact_id` (string), `type` (string) |
| `graphit_hub_install` | Instala um artefato (skill, regra, comando) no projeto local para a IDE. | `artifact_id` (string), `alias` (string) |
| `graphit_hub_uninstall` | Remove um artefato instalado do diretório do projeto. | `artifact_id` (string) |
| `graphit_hub_update` | Atualiza artefatos instalados localmente para a última versão disponível. | `artifact_id` (string) |

### 📚 Wiki e Conhecimento (`graphit_knowledge_` e `graphit_wiki_`)
Ferramentas de busca contextual e conversação sobre especificações do sistema:

| Nome da Ferramenta | Descrição | Parâmetros Principais |
|---|---|---|
| `graphit_knowledge_query` | Executa busca progressiva por IA (multi-turn reader) na wiki de documentação local. | `query` (string), `context` (string) |
| `graphit_knowledge_search` | Executa busca lexical BM25 rápida no índice de documentação local. | `query` (string), `top_k` (int) |
| `graphit_wiki_search` | Realiza pesquisa global integrando wikis locais, memórias e artefatos de terceiros. | `query` (string), `wikis` (array) |
| `graphit_wiki_chat` | Envia mensagem para continuar uma conversa ativa sobre documentações. | `session_id` (string), `message` (string) |
| `graphit_wiki_sessions` | Lista sessões de chat abertas ou exclui sessões antigas. | `action` (string), `session_id` (string) |

### 🧠 Memórias de Agente (`graphit_memory_`)
Ferramentas de leitura e escrita do módulo de memórias de longa-prazo:

| Nome da Ferramenta | Descrição | Parâmetros Principais |
|---|---|---|
| `graphit_memory_query` | Busca memórias semanticamente ou filtra por tags e escopos relevantes. | `query` (string), `scope` (string) |
| `graphit_memory_list` | Lista todas as memórias registradas localmente organizadas por metadados. | `scope` (string) |
| `graphit_memory_add` | Insere uma nova memória de agente no formato e branch correspondente. | `title` (string), `body` (string), `type` (string) |
| `graphit_memory_remove` | Exclui fisicamente um arquivo de memória pelo seu identificador único. | `id` (string) |

---
*Próximo passo recomendado:* Conheça o sistema de [[brand_customization]] dinâmica.
