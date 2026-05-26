---
title: home
updated: 2026-05-26
tags: [knowledge, index]
---

# Wiki de Conhecimento do Graphit Code

> Bem-vindo à wiki de conhecimento do **Graphit Code** (AI Harness for Collaborative and Progressive Knowledge). Esta wiki serve como a fonte central de verdade para entender a arquitetura do sistema, configurações, módulos principais e fluxos de colaboração.
> 
> Explore as seções abaixo e navegue usando os links da wiki para aprofundar-se nos detalhes de cada componente.

## Sumário de Documentos

### 🚀 Primeiros Passos & Configuração
- [[setup_guide]] — Instalação do binário, setup inicial interativo, arquivos de configuração global/projeto, variáveis de ambiente e resolvedores de CLI/IDE.
- [[security_privacy]] — Detalhes sobre a arquitetura local-first e offline-first do Graphit Code, garantindo privacidade absoluta dos dados.

### 📐 Arquitetura do Sistema
- [[overview]] — Visão geral da arquitetura de software, comunicação cliente-daemon-UI e repositório de dados embarcado.
- [[mcp_server]] — Lista detalhada de ferramentas registradas no servidor MCP (Model Context Protocol).

### ⚙️ Módulos Principais
- [[ai_module]] — Detalhes do cliente de IA unificado, fallbacks de CLI (Gemini, Claude, Codex, Kiro, etc.) e gerenciamento de contexto para prompts longos.
- [[ast_module]] — O grafo de conhecimento de código (AST) alimentado por parsers Tree-sitter, armazenamento com LadybugDB local e opções de busca (FTS, Semântica e Query Cipher).
- [[wiki_module]] — Motor de busca inteligente e chat sobre documentação (Wiki), com paginação de leitura orientada por IA, BM25 e injeção de backlinks.
- [[memory_module]] — Módulo de memórias persistentes divididas por escopo (Usuário, Projeto e Contexto) organizadas em formato Markdown estruturado com versionamento Git.

### 🤝 Customização & Colaboração
- [[brand_customization]] — Customização dinâmica de marca (brand branding), prefixos de variáveis de ambiente, arquivos lock e marcadores de hooks.
- [[hub_collaboration]] — O hub descentralizado de conhecimento para compartilhamento de skills, regras, prompts e esquemas de AST determinísticos.
- [[cluster_microservices]] — Agrupamento lógico de ecossistemas de projetos (clusters), geração de `cluster.lock.json` e auto-descoberta de serviços e dependências.

### 🖥️ Interface de Usuário (UI)
- [[ui_interface]] — Visão detalhada do painel de controle React SPA, explorador tridimensional da AST (canvas 3D D3 force directed), chat Wiki e gerenciador do Hub.

---
*Gerado automaticamente pelo motor de conhecimento do Graphit Code.*
