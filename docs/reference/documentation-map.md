---
title: Documentation map
description: Domain navigation for user guides, architecture, detailed contracts and historical qualification records.
content-type: reference
audience: developers
updated: 2026-10-29
---

# Documentation map

Use the [documentation home](../README.md) for journeys and the [system contracts](../architecture/system_contracts.md)
for the reconstruction map. This index makes the detailed sources discoverable; it does not turn
a historical receipt or a proposed design into a supported product interface. Current usage belongs
to guides and the module contracts they link. Dated evidence retains its original execution scope.

## Entry points

- [Repository overview](../../README.md)
- [Contributing and checks](../../CONTRIBUTING.md)
- [Documentation home](../README.md)
- [Configuration keys and defaults](configuration-keys.md)
- [Complete CLI options](cli-options.md)
- [Root MCP input schemas](mcp-input-schemas.json)

## User and operator guides

- [Agent Hook Activation](../guides/agent_hook_activation.md) — Required trust, activation, reload, and verification steps for every Graphit adapter.
- [AI Models, Providers, and Agent CLIs](../guides/ai_models.md)
- [AST grammars and parser extensibility](../guides/ast_extensibility.md)
- [Graphit Broker](../guides/auth-broker.md) — End-to-end provider, login, OIDC, ACL, S3 exchange, embedding, rerank, deployment, and troubleshooting guide.
- [Authentication providers and account profiles](../guides/authentication.md)
- [Capability and Surface Matrix](../guides/capability_matrix.md)
- [CLI Command Reference](../guides/cli_reference.md) — Commands available in the current Rust executable and its productive setup journey.
- [Investigate code relationships](../guides/code_investigation.md)
- [Configuration Reference](../guides/configuration.md) — Rust configuration commands, typed settings, layers, scopes, module switches and domain configuration references.
- [Run Graphit in a Linux container](../guides/container.md)
- [Daemon Operations and Monitoring](../guides/daemon_operations.md)
- [Filesystem, State, and Watchers](../guides/filesystem_contract.md)
- [Getting Started](../guides/getting_started.md) — Install the Rust executables, initialize a project and publish its native agent integration.
- [Publish native artifacts from CI](../guides/github-actions-artifacts.md)
- [Ignore Files](../guides/ignore_files.md)
- [Indexing, incremental updates and versions](../guides/indexing.md) — Progressive availability, compatible embedding reuse, full/incremental indexing, branches and on-the-fly contexts.
- [MCP Tools Reference](../guides/mcp_tools_reference.md) — Complete reference of all MCP tools available to AI agents through Graphit Code.
- [OpenID Connect through Graphit Broker](../guides/oidc-integration.md)
- [Private Branding and Deployment](../guides/private_brand_customization.md)
- [Product screenshots](../guides/product_screenshots.md)
- [Choose a retrieval operation](../guides/retrieval_architecture.md)
- [S3 STS storage and UI network configuration](../guides/s3-and-ui-network.md)
- [Coordinate and resume Task sessions](../guides/task-sessions.md) — Preserve a complete request, decisions and progress across agents using durable Task sessions.
- [Troubleshooting](../guides/troubleshooting.md) — Diagnose and resolve common issues with Graphit Code — daemon, AST indexing, AI/embedding, memory, hub, MCP, and configuration problems.
- [User manual](../guides/user_manual.md)

## Architecture and data flows

- [Adapter hook enforcement](../architecture/adapter-hook-enforcement.md)
- [System Architecture Overview](../architecture/architecture_overview.md) — Current Rust process topology, data planes, storage, authorization and module ownership.
- [Rust agent instruction templates](../architecture/rust-agent-instruction-templates.md)
- [Rust AI Plain with the native ProcessHost](../architecture/rust-ai-plain-native-process-host.md)
- [Rust Dream consolidation instructions](../architecture/rust-dream-instructions.md)
- [Consulta documental e LiveSearch na migração Rust](../architecture/rust-knowledge-ai-live.md)
- [Referências authored de documentos Knowledge em Rust](../architecture/rust-knowledge-authored-references.md) — Adapter puro e limitado entre metadata documental admitida e referências lógicas Records.
- [Fingerprint de compilação documental em Rust](../architecture/rust-knowledge-compilation-fingerprint.md) — Receita JSON compacta limitada sobre metadados admitidos e corpo capturado, com SHA-256 incremental e requisição original.
- [Fronteiras Knowledge, Data e Records na migração Rust](../architecture/rust-knowledge-data-boundaries.md)
- [Parser documental Knowledge em Rust](../architecture/rust-knowledge-document-parser.md) — Sintaxe YAML e Markdown com fonte original, limites explícitos e contexto Core.
- [Projeção documental Knowledge pura em Data Rust](../architecture/rust-knowledge-document-projection.md) — Valores authored emprestados, member rows limitadas e contexto fornecido para consumidores Knowledge.
- [Fonte virtual nativa de documentos Knowledge](../architecture/rust-knowledge-document-source.md)
- [Publicação Knowledge no ForeGround Rust](../architecture/rust-knowledge-foreground.md)
- [Política padrão Knowledge de ignores em Rust](../architecture/rust-knowledge-ignore-policy.md)
- [Locator local de Knowledge Native importada](../architecture/rust-knowledge-import-locator.md) — Seleção de instalação e geração SDK para leitura readonly de Knowledge Native registrada globalmente.
- [Exportação Markdown nativa de Knowledge em Rust](../architecture/rust-knowledge-markdown-export.md)
- [Prévia Markdown Knowledge em Rust](../architecture/rust-knowledge-markdown-preview.md)
- [Publicação autenticada da exportação textual Knowledge](../architecture/rust-knowledge-markdown-publication.md) — Perfis de texto para PROJECT e Knowledge instalado em diretório externo ausente, com autoridade real e publicação conhecida.
- [Mapeamento nativo de documentos para Knowledge em Rust](../architecture/rust-knowledge-native-document-mapping.md) — Composição pura de documentos compilados, identidade lógica e destinos Markdown para a projeção Knowledge.
- [Primeira publicação nativa de documentos Knowledge](../architecture/rust-knowledge-native-publication.md)
- [Exportação Knowledge em OKF e Obsidian](../architecture/rust-knowledge-okf-export.md) — Perfis textuais com proveniência observada, metadata nativa e navegação gerada.
- [Preview de arquivo PROJECT capturado em Rust](../architecture/rust-knowledge-project-preview.md) — Composição de captura conhecida, parser e preview sob a requisição original.
- [Metadados editoriais de captura nativa em Rust](../architecture/rust-knowledge-publication-metadata.md) — Compilação editorial de FILE retido ou buffer externo validado pelo Core, com borrowing e requisição original.
- [Lint do snapshot Knowledge publicado em Rust](../architecture/rust-knowledge-published-lint.md) — Diagnóstico integral do corpus publicado com metadados, referências, corpo curto e staleness observado, sem nova captura de fontes.
- [Diagnóstico de metadados publicados Knowledge em Rust](../architecture/rust-knowledge-published-metadata-lint.md) — Conformidade de kind, title e description integral no corpus FULL16, com limites e erros causais.
- [Consulta documental Knowledge com Codex](../architecture/rust-knowledge-question.md)
- [Persistência nativa de referências Knowledge em Rust](../architecture/rust-knowledge-reference-storage.md) — Estado documental e witnesses íntegros no corpus nativo Knowledge, preservando os DTOs públicos de 16 campos.
- [Retirada explícita de versão Knowledge](../architecture/rust-knowledge-retirement.md)
- [Dados semânticos Knowledge em Rust](../architecture/rust-knowledge-semantic-data.md)
- [Projeção semântica Knowledge em Rust](../architecture/rust-knowledge-semantic-pipeline.md)
- [Configuração tipada Knowledge em Rust](../architecture/rust-knowledge-settings.md) — Resolução pura de docs_dir, README e extensões com origem, limites e contexto Core.
- [Leitura autenticada de fonte Knowledge no App](../architecture/rust-knowledge-source-admission.md)
- [Avaliação documental Knowledge com referências e staleness em Rust](../architecture/rust-knowledge-staleness-projection.md) — Composição de rows documentais com kernels reais e clock explícito.
- [Knowledge e Wiki na migração Rust](../architecture/rust-knowledge-wiki.md) — Contratos documentais ativos, produtores Rust e fronteiras de integração.
- [Rust policy instructions](../architecture/rust-policy-instructions.md)
- [Rust skill instructions](../architecture/rust-skill-instructions.md)
- [Storage Layout](../architecture/storage_layout.md)
- [System contracts and reconstruction map](../architecture/system_contracts.md) — Observable Rust contracts, data and state flows, authority, errors, consumers and implementation owners.

## Detailed contracts

- [Agent runtime context output](../specs/agent_runtime_context.md)
- [AI Engine Specification](../specs/ai_engine.md) — Provider/profile resolution, local/direct/broker embedding, rerank, compatibility, and daemon contracts.
- [AST Module Specification](../specs/ast_module.md) — Technical specification of the AST database, detailing LadybugDB, Cypher translation rules, parser adapters, and indexing pipelines.
- [Broker PROJECT authorization for native Records reads](../specs/auth_broker_project_records.md) — Locally verified Broker callers and fresh PROJECT decisions bound to the original request and actual local captures.
- [Rust authentication provider and profile data](../specs/auth_profiles.md)
- [Typed private authentication storage](../specs/auth_store.md)
- [Bounded HTTP transport](../specs/bounded_http_transport.md)
- [Broker client identity verification](../specs/broker_client_verification.md)
- [Broker inventory service credentials](../specs/broker_inventory_service_credentials.md)
- [Broker native login credential verification](../specs/broker_native_login_verification.md)
- [Broker native OAuth protocol](../specs/broker_native_oauth.md)
- [Captured source metadata](../specs/captured_source_metadata.md)
- [Captured source ranges](../specs/captured_source_ranges.md)
- [Cluster Discovery Specification](../specs/cluster_microservices.md) — Project discovery metadata, current inventory interfaces and the boundary of cluster discovery support.
- [Config Module Specification](../specs/config_module.md) — Rust configuration CLI, typed documents, resolution, scope, safe inspection and atomic writes.
- [Daemon Module Specification](../specs/daemon_module.md) — Technical specification of the background Daemon service, project supervisors, OS schedulers, and replacement lifecycles.
- [Evolution, ownership and validation](../specs/design_system/evolution.md)
- [Foundations and tokens](../specs/design_system/foundations.md)
- [Components and interaction patterns](../specs/design_system/patterns.md)
- [Workspaces and page contracts](../specs/design_system/workspaces.md)
- [Graphit design system](../specs/design_system.md)
- [Dream Module Specification](../specs/dream_module.md) — Technical contract for idle-triggered, agentic Memory consolidation.
- [Embedded Language Parsing](../specs/embedded_language_parsing.md) — How the body of <script> and <style> in single-file components is parsed with the grammar of the language it is written in. Implemented for Vue, Svelte and HTML.
- [Git Module Specification](../specs/git_module.md) — System Git checkout/hooks and native record snapshots, Role gix reads, process ownership and configured-hook contracts.
- [Task and Memory snapshots for Git commits](../specs/git_records.md)
- [Hub S3 object layout](../specs/hub-s3-object-layout.md) — The versioned S3 key and document contract for project identity, discovery, access control, artifacts, and mutable project state.
- [Hub Access Control](../specs/hub_access_control.md) — Deny-by-default project authorization, selective discovery, and cache rules for the S3-backed Hub.
- [Hub Collaboration Specification](../specs/hub_collaboration.md)
- [Rust Hub caller scopes, bindings and resource policy](../specs/hub_consumer_contract.md)
- [Rust native installation identity](../specs/hub_native_installations.md)
- [Rust Hub resource policy](../specs/hub_resource_policy.md)
- [Outgoing Hub read service credentials](../specs/hub_service_credentials.md)
- [Outgoing Hub write service credentials](../specs/hub_write_service_credentials.md)
- [Leitura local selecionada de página Knowledge](../specs/knowledge_index_page.md) — Página Knowledge local autenticada, com corpo e proveniência do mesmo pin e validação selecionada até EOF.
- [Leitura selecionada Knowledge de um snapshot SDK](../specs/knowledge_module_snapshot.md)
- [Captured local Lance branch staging](../specs/lance_native_branch_clone.md)
- [Pinned local Lance SDK clone](../specs/lance_native_clone.md)
- [Pinned Lance native metadata](../specs/lance_native_metadata.md)
- [Lance native snapshot opener](../specs/lance_native_snapshot.md)
- [Managed role context](../specs/managed_role_context.md)
- [Managed Runtime manager state and turn fences](../specs/managed_runtime_manager_state.md)
- [MCP Stdio Module Specification](../specs/mcpstdio_module.md) — Technical specification of the MCP stdio server, tool registration, JSON-RPC message flow, request context resolution, and error handling.
- [Memory Module Specification](../specs/memory_module.md)
- [Artifact catalogue writer](../specs/native_artifact_writer_request.md)
- [Native Auth CLI command component](../specs/native_auth_cli.md) — Provider, Local profile and native Broker OAuth commands with caller-owned administrative authority.
- [Native Broker Device login](../specs/native_broker_device_login.md)
- [Native Broker interactive login](../specs/native_broker_interactive_login.md)
- [Native Broker profile login](../specs/native_broker_profile_login.md)
- [Native Broker profile refresh](../specs/native_broker_profile_refresh.md)
- [Rust native document codec](../specs/native_document_codec.md)
- [Captured Icebug publication metadata](../specs/native_graph_metadata.md) — Borrowed file and manifest generation references from the existing validated graph capture.
- [Native installation authority](../specs/native_installation_authority.md)
- [Rust Native package installation ingress](../specs/native_installation_ingress.md)
- [Embeddings Memory PROJECT nativos: contrato planejado](../specs/native_memory_embeddings.md)
- [Índice lexical Memory PROJECT nativo](../specs/native_memory_search.md)
- [Native PROJECT Memory creation and current metadata](../specs/native_memory_store.md)
- [Native process principal observation](../specs/native_process_principal.md)
- [Native profile manager](../specs/native_profile_manager.md)
- [PROJECT document reads and owner-root records](../specs/native_project_document_capture.md)
- [Captured Project and Installation roots](../specs/native_scope_capture.md)
- [Shared native host document transactions](../specs/native_shared_documents.md)
- [Native Task textual and semantic search](../specs/native_task_search.md)
- [Native Task ownership and read views](../specs/native_task_store.md)
- [Native USER Memory model consumer](../specs/native_user_memory_embeddings.md) — USER admissions composed with the selected embedding service and native Memory SDK.
- [Native USER Memory data contracts and layout](../specs/native_user_memory_store.md) — Literal USER owner, shared Memory result DTOs and exact Arrow25 layout; storage qualification is scoped separately.
- [Native USER namespace capture](../specs/native_user_scope.md)
- [Original request context across crate boundaries](../specs/original_request_context.md)
- [Output Module Specification](../specs/output_module.md) — Technical specification of the decoupled view/output layer: the Printer abstraction, view interfaces, formatters, structured output for CLI and MCP modes.
- [Presence service credentials](../specs/presence_service_credentials.md)
- [Private installation authentication document capture](../specs/private_auth_document.md)
- [Private installation directory bootstrap](../specs/private_installation_bootstrap.md)
- [Project Identity](../specs/project_identity.md) — The lifecycle and invariants of the stable project ULID and mutable globally unique project name.
- [Project record admission](../specs/project_record_admission.md)
- [Captured PROJECT source](../specs/project_source_capture.md)
- [Persisted record relationships](../specs/record_relations.md) — Typed relationships across Task, Session, Memory and Knowledge, including authoring, queries and recovery.
- [Rust remote artifact metadata](../specs/remote_artifact_metadata.md)
- [Remote artifact storage](../specs/remote_artifact_storage.md) — Current keys, file payload integrity and publication boundary shared by the Rust Hub and Broker.
- [Rust remote artifact version selection](../specs/remote_artifact_version_selection.md)
- [Remote file artifact installation](../specs/remote_file_install.md) — Ordinary ZIP staging, streaming retrieval and productive Hub FILE consumers.
- [Remote Icebug graph queries](../specs/remote_icebug_graph.md)
- [Remote native root and GlobalNative locator](../specs/remote_native_root.md) — Exact native descriptor integrity and explicit remote registry dispatch for direct SDK mounts.
- [Role credentials](../specs/role_credentials.md)
- [Rule Override Specification](../specs/rule_override.md) — Technical specification of the multi-layer rule and skill override system, covering project-level, global CLI, and Hub-based overrides with placeholder substitution.
- [Private runtime key ownership](../specs/runtime_key.md)
- [Rust Antigravity Knowledge Ask and Conversation](../specs/rust_antigravity_completion.md)
- [Rust ANTLR recognizers and preprocessing](../specs/rust_ast_antlr.md) — The native recognizer APIs, source provenance and regeneration recipe for the maintained ANTLR families.
- [Rust AST Oracle semantic model](../specs/rust_ast_antlr_model.md) — Genuine Oracle declaration identities, owners, values and original code projections.
- [Rust AST ANTLR profile queries](../specs/rust_ast_antlr_profile_queries.md) — Guards and nearest captures over official ANTLR trees.
- [Rust AST ANTLR profile reconciliation](../specs/rust_ast_antlr_profile_reconciliation.md) — Native grammar mappings, delivered profile capabilities and historical defects.
- [Rust AST ANTLR queries](../specs/rust_ast_antlr_queries.md) — Native XPath selection over the live ANTLR parse tree.
- [Rust AST ANTLR semantic occurrences](../specs/rust_ast_antlr_semantics.md) — Effective profiles bound to native captures and original origins.
- [Rust original entity extent](../specs/rust_ast_entity_extent.md)
- [Rust entity Source window selection](../specs/rust_ast_entity_source.md)
- [Rust AST Grammar Provenance and Language Lifetimes](../specs/rust_ast_grammar_provenance.md) — Source inventory, artifact boundaries and qualification requirements for all Tree-sitter grammars.
- [Policy Rust de ignores AST](../specs/rust_ast_ignore_policy.md)
- [Local AST graph by FILE](../specs/rust_ast_live_graph.md) — Icebug Parquet FILE updates and lazy native queries without waiting for embeddings.
- [Local progressive AST search](../specs/rust_ast_live_search.md)
- [Rust AST parser resources](../specs/rust_ast_parser_resources.md) — Linked syntax grammar owners, prepared profile factories and custom Docker SCIP producers.
- [Rust AST and Parser Contracts](../specs/rust_ast_parsers.md) — Source ranges and the producer boundaries for the Rust AST migration.
- [Rust AST Profile Catalog](../specs/rust_ast_profile_catalog.md) — Declarative catalog, overlays, provenance, routing and input limits.
- [Rust AST Profile Decoder](../specs/rust_ast_profiles.md) — Typed YAML profile schema, original locations, input limits and decoder API.
- [Rust AST Raw foreground Source](../specs/rust_ast_raw_foreground.md) — Native profile selection and per-file streaming of Raw Source in the project supervisor.
- [Rust AST semantic extraction and publication](../specs/rust_ast_semantic_extraction.md)
- [Native AST Graph producer](../specs/rust_ast_semantic_graph.md) — Owned per-file Graph observations from genuine semantic models and Source, independent of NN inference.
- [Rust AST JSON semantic model](../specs/rust_ast_semantic_model.md)
- [Rust authenticated local Source handler](../specs/rust_ast_source_handler.md)
- [Rust local Source index acquisition](../specs/rust_ast_source_index.md)
- [Rust AST Symbols and Identity](../specs/rust_ast_symbols.md) — Rust root/exact identities, original ranges, ownership and bounded serialization.
- [Rust Tree-sitter engine](../specs/rust_ast_tree_sitter.md)
- [Active daemon Auth administration](../specs/rust_auth_active_admin.md) — Native typed Auth commands on the retained daemon issuer and listener.
- [Installation-owned Auth entry component](../specs/rust_auth_entrypoint.md)
- [Rust browser OAuth sessions](../specs/rust_browser_auth.md)
- [Captured local Cypher](../specs/rust_captured_cypher.md)
- [Explicit local checkout retirement](../specs/rust_checkout_retirement.md)
- [Rust Claude one-shot completion](../specs/rust_claude_completion.md)
- [Rust CLI help](../specs/rust_cli_help.md) — Public help, compiled version and native package commands through one Clap parser.
- [Rust CLI model installation and selection](../specs/rust_cli_models.md) — Typed model commands in the Rust executable with native installation paths and cancellation.
- [Rust incremental plain CLI text](../specs/rust_cli_plain.md) — Strict bounded UTF-8 text from the original Process invocation.
- [Rust CLI version](../specs/rust_cli_version.md)
- [Rust Codex completion stream](../specs/rust_codex_completion.md) — Consume the pinned Codex JSONL protocol incrementally through the existing ProcessHost and caller's original context.
- [Rust Cohere text embeddings](../specs/rust_cohere_embeddings.md) — Explicit positional Cohere embeddings with bounded transport and the original execution context.
- [Rust Cohere native reranking](../specs/rust_cohere_rerank.md)
- [Rust Copilot one-shot Knowledge Ask](../specs/rust_copilot_completion.md) — Contrato Copilot CLI JSONL com custódia Process, correlação nativa, limites e fixtures locais.
- [Rust Cursor one-shot Knowledge Ask](../specs/rust_cursor_completion.md)
- [Rust daemon authentication startup](../specs/rust_daemon_auth_startup.md)
- [Daemon Broker verifier ownership](../specs/rust_daemon_broker_verifiers.md)
- [Rust daemon memory and lifecycle contract](../specs/rust_daemon_memory_lifecycle.md) — Planned Rust daemon ownership, shared resources and incremental multi-project journeys.
- [Rust dedicated model child](../specs/rust_daemon_model_child.md) — Private finite credential bootstrap, real model RPC and owned process cleanup.
- [Desconexão do transporte de modelos em Rust](../specs/rust_daemon_model_transport.md)
- [Serviços do usuário em Rust](../specs/rust_daemon_services.md)
- [Supervisão de checkouts em Rust](../specs/rust_daemon_watchers.md)
- [Rust Direct embedding service leases](../specs/rust_direct_embedding_leases.md) — Compose six literal Direct embedding protocols with the real Auth lease and existing SDKs.
- [Rust Direct native reranking with Auth admission](../specs/rust_direct_native_rerank.md)
- [Rust Dream records](../specs/rust_dream_records.md)
- [Rust JavaScript, TypeScript and TSX semantic model contract](../specs/rust_ecma_semantic_model.md) — Retained native ECMAScript model, canonical name recipe and consumer boundaries.
- [Leituras Knowledge PROJECT no browser Rust](../specs/rust_foreground_knowledge.md) — Contrato de heads e página nativa por seleção exata, com autorização Knowledge própria e saída limitada.
- [Memory PROJECT no browser Foreground Rust](../specs/rust_foreground_memory.md) — Leitura, escrita e busca Memory PROJECT e própria USER por sessão browser e admissões nativas.
- [Referências nativas no browser Foreground Rust](../specs/rust_foreground_record_references.md) — Fontes independentes, autoridade por módulo e continuação nativa das relações entre registros.
- [Task browser no serviço Foreground Rust](../specs/rust_foreground_tasks.md) — Configuração, autenticação, leitura PROJECT e encerramento Task no listener Foreground existente.
- [Rust Gemini completion stream](../specs/rust_gemini_completion.md)
- [Rust paired GGUF reranking](../specs/rust_gguf_rerank.md) — BGE v2-m3 F16 paired classification with bounded native execution.
- [Rust Google text embeddings](../specs/rust_google_embeddings.md) — Explicit Google embedding families, single and batch requests with bounded transport and positional vectors.
- [Rust Grok Knowledge Ask and Conversation](../specs/rust_grok_completion.md)
- [Captured WebSocket transport](../specs/rust_http_websocket.md)
- [Borrowed Icebug schema](../specs/rust_icebug_schema.md)
- [Captured Icebug storage API](../specs/rust_icebug_storage.md)
- [Leitura readonly de Knowledge Native importada](../specs/rust_imported_knowledge.md) — Abertura de artifact Knowledge registrado globalmente com admissão Web real, pin SDK e leitura protegida até EOF.
- [Consulta de referências Knowledge Native importada](../specs/rust_imported_reference_query.md) — Páginas readonly da pin importada admitida, com namespace persistido, cobertura explícita e cursor da seleção observada.
- [Rust imported Source](../specs/rust_imported_source.md)
- [Rust direct inference library](../specs/rust_inference.md) — Explicit OpenAI-shaped embedding requests with bounded transport, vectors and execution context.
- [Installed native AST graph reads](../specs/rust_installed_graph.md)
- [Rust Jina reranking adapter](../specs/rust_jina_rerank.md)
- [Rust Kimi REST/WS Ask and Conversation](../specs/rust_kimi_completion.md) — Contratos Kimi para Knowledge Ask e Incoming Conversation, com custódia Process e sessão/prompt/turno nativos.
- [Rust Kimi Print for managed Role execution](../specs/rust_kimi_print.md) — Kimi Print JSONL consumido sobre ProcessHost, com recheck concreto e sem retenção paralela de saída.
- [Rust Kiro ACP completion](../specs/rust_kiro_completion.md) — One SDK ACP prompt under the original execution context, with bounded consumer retention and real protocol receipts.
- [Rust Kiro hook plans](../specs/rust_kiro_hook_plans.md)
- [Primeira publicação Knowledge com metadata authored em Rust](../specs/rust_knowledge_authored_first_publication.md) — Criação exclusiva de documentos compilados com projeções de referência completas ou incompletas na mesma imagem.
- [Materialização explícita de branch Knowledge em Rust](../specs/rust_knowledge_branch_materialization.md) — Criar a ref de uma branch herdada após admissions, geração esperada e validação Full18 até EOF.
- [Rust USER knowledge conversation storage](../specs/rust_knowledge_conversation.md)
- [Rust Incoming MCP Knowledge conversations](../specs/rust_knowledge_conversation_app.md)
- [Corpus desejado capturado Knowledge em Rust](../specs/rust_knowledge_desired_corpus.md)
- [Reconciliação da seleção completa Knowledge em Rust](../specs/rust_knowledge_desired_reconciliation.md)
- [Domínios e comunidades documentais Knowledge em Rust](../specs/rust_knowledge_document_domains.md) — Classificação topológica do grafo completo com report owned e integração de exportação separada.
- [Reconciliação de documentos Knowledge em Rust](../specs/rust_knowledge_document_lifecycle.md)
- [Republicação de documentos Knowledge existentes em Rust](../specs/rust_knowledge_document_republication.md) — Captured compiled inputs, autoridade dupla, imagem física completa e CAS sem retry.
- [Consulta nativa de referências Knowledge em Rust](../specs/rust_knowledge_reference_query.md) — Proveniência literal, cobertura explícita e páginas vinculadas à geração observada.
- [Busca Knowledge em Rust](../specs/rust_knowledge_search.md)
- [Primeira publicação Knowledge com Source revalidada em Rust](../specs/rust_knowledge_source_first_publication.md)
- [Identidade das fontes Knowledge em Rust](../specs/rust_knowledge_source_identity.md)
- [Primeira publicação da seleção Knowledge em stream Rust](../specs/rust_knowledge_source_stream.md)
- [Ladybug SDK for local graph queries](../specs/rust_ladybug_sdk.md)
- [Lance branch delta producer](../specs/rust_lance_branch_store.md)
- [Integridade e limitações do SDK Lance](../development/lance-upstream.md#integrity-limits)
- [Store Lance por módulo](../specs/rust_lance_module_store.md)
- [Named Lance branch retirement](../specs/rust_lance_named_branch_retirement.md)
- [MCP USER de leitura para runs Live Rust](../specs/rust_live_agent_tools.md)
- [Rust local GGUF model clients](../specs/rust_local_models.md) — Local GGUF execution, model recipes and caller-owned context.
- [Rust local GGUF execution](../specs/rust_local_runtime.md) — Local GGUF execution, model recipes and caller-owned context.
- [Rust Memory attribute aggregation](../specs/rust_memory_attributes.md)
- [Explicit Memory index maintenance](../specs/rust_memory_index.md) — Authoritative PROJECT and verified USER Memory index maintenance through the Rust CLI and Source MCP.
- [Rust Memory records](../specs/rust_memory_records.md)
- [Captured GGUF model bindings](../specs/rust_model_bindings.md)
- [Rust GGUF model backends](../specs/rust_model_manifest.md)
- [Rust shared model service](../specs/rust_model_service.md) — Captured local model selection, lazy owners and authenticated private inference RPC.
- [Native EmbeddingGemma2 SDK library](../specs/rust_native_embedding.md)
- [Native embedding worker](../specs/rust_native_embedding_worker.md)
- [Rust native publication](../specs/rust_native_publication.md) — Captured native snapshots, credential boundaries and conditional remote publication.
- [Rust private native rerank wire pipeline](../specs/rust_native_rerank_wire.md)
- [Rust native reranking validation](../specs/rust_native_reranking.md)
- [Native storage object locations](../specs/rust_native_storage_uri.md) — Current S3 object namespaces for native project, user and Hub stores.
- [CLI Now contínua em Rust](../specs/rust_now_cli.md)
- [Rust Now and Presence integration](../specs/rust_now_presence_integration.md)
- [Rust Now PROJECT head sources](../specs/rust_now_project_heads.md)
- [Rust OpenCode completion](../specs/rust_opencode_completion.md)
- [Rust Linux detached startup custody](../specs/rust_process_detached.md)
- [Original-context Rust subprocess execution](../specs/rust_process_incremental.md)
- [Rust service child supervision](../specs/rust_process_service.md)
- [Bootstrap de projeto Runtime Rust](../specs/rust_project_bootstrap.md) — Observação do checkout e registro da seleção no inventário Core.
- [Rust PROJECT Dream caller](../specs/rust_project_dream.md)
- [Rust PROJECT hook context](../specs/rust_project_hook_context.md)
- [Rust maintained project inventory](../specs/rust_project_inventory.md) — Local project and version facts consumed independently of online presence.
- [Rust Qwen one-shot completion](../specs/rust_qwen_completion.md)
- [Functional Rust record gates](../specs/rust_record_gates.md)
- [Rust record transaction boundary](../specs/rust_record_transactions.md)
- [Rust ranking and inference context](../specs/rust_reranking.md) — Pure ranking primitives over caller-owned vectors and scores; distinct from the configured product reranker.
- [Local Role MCP toolbox](../specs/rust_role_mcp.md)
- [Rust Root module skill](../specs/rust_root_module_skill.md)
- [Rust Runtime own Broker inventory](../specs/rust_runtime_broker_inventory.md)
- [Runtime Knowledge publication and maintained inventory](../specs/rust_runtime_inventory.md)
- [Rust Runtime maintained inventory snapshot](../specs/rust_runtime_maintained_snapshot.md)
- [Rust Runtime Now publisher](../specs/rust_runtime_now_publisher.md)
- [Runtime Now subscription](../specs/rust_runtime_now_subscription.md)
- [Native Runtime PROJECT Now composition](../specs/rust_runtime_project_now.md)
- [Rerank opt-in de uma página de busca em Rust](../specs/rust_search_rerank_consumer.md)
- [Rust installed self-update](../specs/rust_self_update.md)
- [Rust self-update release preparation](../specs/rust_self_update_preparation.md)
- [Clientes Rust de Source](../specs/rust_source_callers.md)
- [Rust Source service discovery](../specs/rust_source_discovery.md)
- [Entrada Rust de Source](../specs/rust_source_entrypoint.md)
- [Source foreground em Rust](../specs/rust_source_foreground.md)
- [Rust authenticated Source HTTP MCP component](../specs/rust_source_mcp.md)
- [Now e instâncias no Source Rust](../specs/rust_source_now.md)
- [Native Task calendar and record views](../specs/rust_task_gantt.md) — Authored Task dates, native working contexts and read-only record views in React.
- [Authenticated native Task MCP and HTTP component](../specs/rust_task_mcp.md)
- [Native Task protocol](../specs/rust_task_protocol.md)
- [Rust Task specification transitions](../specs/rust_task_specification.md)
- [Rust Core text selection](../specs/rust_text_selection.md)
- [Rust Tree-sitter native contexts](../specs/rust_tree_sitter_contexts.md) — Native context/name queries, cursor scopes and original-source projections over one collection.
- [Rust generic Tree-sitter semantic model](../specs/rust_tree_sitter_generic_model.md)
- [Rust Tree-sitter entities in native NN](../specs/rust_tree_sitter_native_nn.md) — Genuine Rust and stock Go original projections, Source associations and observed embeddings in native NN.
- [Rust Tree-sitter effective profile matches](../specs/rust_tree_sitter_profile_matches.md)
- [Rust Tree-sitter Struct and Field semantic model](../specs/rust_tree_sitter_semantic_model.md) — Genuine Rust and stock Go declarations, native literal Source and local evidence resolution from one context collection.
- [Contrato Rust de UI, MCP/proxy e tray](../specs/rust_ui_mcp_tray_contract.md)
- [Exact cosine over variable-width vectors](../specs/rust_vector_distance.md)
- [Rust Voyage text embeddings](../specs/rust_voyage_embeddings.md) — Native-width Voyage embeddings with indexed responses, bounded transport and original execution context.
- [Rust Voyage native reranking](../specs/rust_voyage_rerank.md)
- [Rust Web OAuth protocol](../specs/rust_web_oauth.md)
- [Per-call service credentials](../specs/service_credentials.md)
- [Task Module Specification](../specs/task_module.md)
- [Explicit Task calendar dates](../specs/task_schedule.md)
- [Task Sessions contract](../specs/task_sessions.md)
- [Task timeline renderer](../specs/task_timeline_renderer.md)
- [UI Dashboard Specification](../specs/ui_dashboard.md)
- [USER Memory and Knowledge record admission](../specs/user_record_admission.md)
- [Verified literal user identity](../specs/verified_user_subject.md)
- [Web Hub mutation authority](../specs/web_hub_mutation_authority.md) — Installation policy and retained Web authority for FILE effects and publication.
- [Web imported Knowledge read admission](../specs/web_imported_knowledge_admission.md) — Borrowed Web proof and physical installation authority, independent of USER and PROJECT grants.
- [Web installed Hub read admission](../specs/web_installed_hub_admission.md) — Explicit Web authority for installed Agent, Skill and Rule readers.
- [Wiki Module Specification](../specs/wiki_module.md) — Current authored Knowledge compilation, progressive text/vector publication, references, retrieval and AI consumers.

## Development and qualification history

- [Qualificação SDK de armazenamento e NN para embeddings Knowledge](../development/knowledge_embeddings_sdk_qualification.md)
- [Qualificação de FTS JSON e publicação estrita de índices Lance13](../development/lance13_fts_index_qualification.md)
- [Official Lance integration](../development/lance-upstream.md)
- [Install canonical local Hub instructions](../development/local-hub-installation.md)
- [Global native instructions](../development/native-global-instructions.md)
- [Native host bindings during the Rust rewrite](../development/native-host-bindings.md)
- [Native AST and Knowledge packages](../development/native-packages.md)
- [Rust native runtime packaging](../development/native-runtime-packaging.md) — Build pinned native SDKs and package the Rust executables and optional accelerators.
- [Publicação de índice FTS JSON no snapshot Data capturado](../development/native_json_fts_index.md)
- [Rust checks and Git hooks](../development/rust-fast-guard.md)
- [Forks Rust e origem dos consumidores](../development/rust-forks.md)
- [Rust product architecture and delivery](../development/rust-migration.md) — Maintained boundaries, product contracts and validation commands for Graphit Rust.
- [Rust Storage Contracts](../development/rust-storage-contracts.md) — Current Rust storage APIs, historical native evidence, and remaining consumer integration.
- [Rust workspace checkout](../development/rust-workspace.md)
- [Web FILE publication from captured PROJECT source](../development/web-file-source-publication.md)
- [Rust workspace integration — 2026-10-09](../development/workspace-members-2026-10-09.md)

## Examples

- [Graphit Remote](../examples/skills/graphit-remote/SKILL.md) — Read admitted Graphit content through the actual MCP catalog without inventing a source checkout or authorization.

## Reference catalogs

- [Complete CLI option reference](../reference/cli-options.md) — Inspected Rust CLI command tree, positional arguments, flags, accepted values and defaults.
- [Configuration keys, defaults and capture boundaries](../reference/configuration-keys.md) — Active keys, value types, defaults, opt-ins, scope and concrete input bounds in the Rust product.

## Maintenance boundaries

A changed contract must update its owning guide/spec, examples and incoming navigation together.
Verify factual claims against the producer and its real consumers; a link/schema check alone does
not qualify runtime behavior. Use the current CLI help and MCP schemas when adding examples.
Preserve known limitations and identify proposed or historical behavior explicitly. Documentation
maintenance does not authorize publishing Hub content, committing or pushing a repository.
