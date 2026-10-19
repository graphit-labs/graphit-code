# Graphit Code Documentation

Graphit is a context and control plane for AI software engineering. It connects structural code,
maintained documentation, persistent memory, deterministic shared tasks, reusable ecosystem
artifacts, and bounded live agent runs. This hub routes developers, operators, and coding agents to
current guidance without mixing it with historical implementation records.

The current visual reference is the [Graphit design system](specs/design_system.md).
See the [code investigation workflow](guides/code_investigation.md) and
[product screenshots](guides/product_screenshots.md) for current UI examples and capture maintenance.

## Choose your path

| I want to… | Start here | Continue with |
|---|---|---|
| Install Graphit and initialize a repository | [Getting Started](guides/getting_started.md) | [CLI Reference](guides/cli_reference.md) |
| Configure local or Broker profiles and browser login | [Authentication](guides/authentication.md) | [Configuration Reference](guides/configuration.md) |
| Integrate an enterprise identity provider end to end | [OIDC Integration](guides/oidc-integration.md) | [Hub Access Control](specs/hub_access_control.md) |
| Expose an authenticated UI or HTTP MCP endpoint | [Container deployment](guides/container.md) | [OIDC Integration](guides/oidc-integration.md) |
| Centralize OIDC/anonymous ACLs, restricted S3 sessions, embeddings and rerank | [Graphit Broker](guides/auth-broker.md) | [Authentication](guides/authentication.md) |
| Use Graphit day to day | [User Manual](guides/user_manual.md) | [Troubleshooting](guides/troubleshooting.md) |
| Let the agent fix failed checks across Git hook events | [Git hook commands](guides/git-hooks.md) | [Git hook installation contract](specs/git_module.md) |
| Understand every setting and feature switch | [Configuration Reference](guides/configuration.md) | [Configuration Specification](specs/config_module.md) |
| Choose an agent CLI, embedding model, or rerank provider | [AI Models, Providers, and Agent CLIs](guides/ai_models.md) | [AI Engine](specs/ai_engine.md) |
| Start, monitor, and troubleshoot the background service | [Daemon Operations and Monitoring](guides/daemon_operations.md) | [Daemon Module](specs/daemon_module.md) |
| Configure an agent or MCP client | [MCP Tools Reference](guides/mcp_tools_reference.md) | [Retrieval Architecture](guides/retrieval_architecture.md) |
| Coordinate resumable work across agents | [Task workflow](guides/user_manual.md#dream-and-task) | [Task Module](specs/task_module.md) |
| Inspect or export complete Task records | [Task workflow](guides/user_manual.md#dream-and-task) | [CLI Reference](guides/cli_reference.md#source-records-and-live-views) |
| Choose between FTS, semantic, hybrid, graph, AI, and live search | [Retrieval Architecture](guides/retrieval_architecture.md) | [AI Engine](specs/ai_engine.md) |
| Add or customize a language parser | [AST Grammars and Parser Extensibility](guides/ast_extensibility.md) | [AST Module](specs/ast_module.md) |
| Share context across repositories and machines | [User Manual](guides/user_manual.md#teams-agents-and-software-ecosystems) | [Hub Collaboration](specs/hub_collaboration.md) |
| Understand project IDs, names, rename, and early identity | [Project Identity](specs/project_identity.md) | [Storage Layout](architecture/storage_layout.md) |
| Design or operate selective Hub access | [Hub Access Control](specs/hub_access_control.md) | [S3 and UI Network](guides/s3-and-ui-network.md) |
| Understand progressive indexing, reuse, versions and on-the-fly reads | [Indexing](guides/indexing.md) | [Retrieval](guides/retrieval_architecture.md) |
| Reimplement or integrate the current contracts | [System contracts](architecture/system_contracts.md) | [Exact CLI options](reference/cli-options.md) and [MCP schemas](reference/mcp-input-schemas.json) |
| Understand system boundaries | [Architecture Overview](architecture/architecture_overview.md) | [Storage Layout](architecture/storage_layout.md) |
| Operate shared storage or networked UI | [S3 and UI Network](guides/s3-and-ui-network.md) | [Hub S3 Object Layout](specs/hub-s3-object-layout.md) |
| Serve a team over MCP from a container | [Run as a Server in a Container](guides/container.md) | [MCP Tools Reference](guides/mcp_tools_reference.md) |
| Publish current AST and knowledge contexts from CI | [Publishing from GitHub Actions](guides/github-actions-artifacts.md) | [Hub Collaboration](specs/hub_collaboration.md) |
| Customize a private distribution | [Private Brand Customization](guides/private_brand_customization.md) | [Configuration Specification](specs/config_module.md) |
| Design or evolve Code, Broker or the public site | [Design system](specs/design_system.md) | [Patterns](specs/design_system/patterns.md) and [evolution checks](specs/design_system/evolution.md) |
| Contribute to the project | [Contributing](../CONTRIBUTING.md) | [Repository README](../README.md) |

## Guides

Guides explain workflows from a user or operator perspective.

- [Getting Started](guides/getting_started.md) — installation, setup, project initialization, first sync, and the Observatory.
- [Indexing](guides/indexing.md) — full/incremental publication, semantic reuse, branches, local/Broker and installed contexts.
- [Authentication providers and account profiles](guides/authentication.md) — named local and Broker providers, login/logout, multiple profiles, MCP/Broker Bearer, dynamic broker storage, and local non-interactive operation.
- [OIDC Integration](guides/oidc-integration.md) — Broker-issued tokens, Authorization Code with PKCE, MCP OAuth discovery and resource binding, dual access-token audiences, and bearer forwarding to Broker APIs.
- [Graphit Broker](guides/auth-broker.md) — provider modes, SQLite/PostgreSQL/MySQL persistence, OIDC/RBAC administration, transactional SQL ACLs, restricted STS issuance, embeddings/rerank, revision rollout, and troubleshooting.
- [User Manual](guides/user_manual.md) — everyday AST, knowledge, memory, Hub, daemon, Dream, and ecosystem workflows.
- [Configuration Reference](guides/configuration.md) — layers, scopes and workflows; [key/default catalog](reference/configuration-keys.md) contains active settings and concrete input bounds.
- [AI Models, Providers, and Agent CLIs](guides/ai_models.md) — completion CLI resolution and protocols, local/remote embeddings, model dimensions, rerank providers, credentials, and data boundaries.
- [Daemon Operations and Monitoring](guides/daemon_operations.md) — startup, schedulers, every monitored signal and loop, project parking, services, runtime files, logs, replacement, and recovery.
- [Capability and Surface Matrix](guides/capability_matrix.md) — implemented modules, CLI/MCP/UI availability, gates, and current boundaries.
- [Filesystem, State, and Watchers](guides/filesystem_contract.md) — special files, generated state, adapter layouts, and change detection.
- [AST Grammars and Parser Extensibility](guides/ast_extensibility.md) — complete YAML schema, selectors, Tree-sitter and ANTLR extension boundaries, distribution, and validation.
- [CLI Command Reference](guides/cli_reference.md) — command journeys and effects; [complete options](reference/cli-options.md) lists the inspected command tree and positional/flag contracts.
- [MCP Tools Reference](guides/mcp_tools_reference.md) — actual current Root tools and output/error rules; [full schemas](reference/mcp-input-schemas.json) preserve nested inputs/defaults/validation.
- [Retrieval Architecture](guides/retrieval_architecture.md) — when to search, read source, traverse a graph, or use live search.
- [Ignore Files](guides/ignore_files.md) — source and documentation exclusion behavior.
- [S3 Credentials and UI Network](guides/s3-and-ui-network.md) — optional remote storage, credentials, binding, CORS, and security boundaries.
- [Run as a Server in a Container](guides/container.md) — the root `Dockerfile`: an MCP endpoint any AI agent can connect to, the daemon as PID 1, and serving Hub artifacts with no checkout on the server.
- [Publishing from GitHub Actions](guides/github-actions-artifacts.md) — non-interactive runtime setup plus provider/login, explicit indexing, and publication to a production Hub.
- [Private Brand Customization](guides/private_brand_customization.md) — branded binaries and private collaboration environments.
- [Troubleshooting](guides/troubleshooting.md) — common operational failures and diagnostics.

## Architecture

- [System Architecture Overview](architecture/architecture_overview.md) — launcher, core engine, modules, daemon, UI, Hub, and Agent adapters.
- [Storage Layout](architecture/storage_layout.md) — global stores, project records, contexts, caches, and published data.

## Module specifications

Specifications describe current module behavior and technical contracts.

- [AST Module](specs/ast_module.md)
- [Wiki Module](specs/wiki_module.md)
- [Memory Module](specs/memory_module.md)
- [Task Module](specs/task_module.md)
- [Hub Collaboration](specs/hub_collaboration.md)
- [Hub S3 Object Layout](specs/hub-s3-object-layout.md)
- [Project Identity](specs/project_identity.md)
- [Hub Access Control](specs/hub_access_control.md)
- [Configuration Module](specs/config_module.md)
- [Daemon Module](specs/daemon_module.md)
- [Dream Module](specs/dream_module.md)
- [UI Dashboard](specs/ui_dashboard.md)
- [AI Engine](specs/ai_engine.md)
- [Cluster Discovery](specs/cluster_microservices.md)

## Historical records

Dated receipts and implementation sections explicitly described as historical preserve their original execution scope. They do not override current user contracts. The documentation separates these from the product entry pages:

- Task history — authoritative LanceDB task/event/comment/check tables, queried through Graphit Task.
- Dated development qualification pages — point-in-time SDK/native validation.
- Historical qualification sections in detailed specs — retained evidence, not current host installation prerequisites.
- Temporary `.build`/evidence files that were removed are identified as unavailable, never recreated or claimed as new proofs.

Historical records may describe superseded behavior. For current usage, prefer guides, architecture pages, and module specifications; follow links into history when you need rationale or provenance.

## Documentation conventions

- Maintained documentation and the root README are written in English.
- User workflows belong in `guides/`; system boundaries in `architecture/`; module contracts in `specs/`.
- Implementation work is created, claimed, checked, commented, and completed through Graphit Task; do not create Markdown task logs.
- Commands and file paths use code formatting; security limitations are stated next to the relevant configuration.
- Screenshots should use the current Graphit Observatory UI and an isolated fictional demonstration context, without private project/account data.

## Keep the wiki current

The daemon watches selected documentation sources and publishes changed pages/text progressively, then attaches compatible embeddings in background batches. Stop hooks request an asynchronous PROJECT rescan. `graphit sync` reconciles exact installed Hub artifacts and native agent files; it neither waits for index completion nor upgrades remote releases.

Use the [documentation map](reference/documentation-map.md) to find every maintained guide, architecture page and detailed specification. The [reconstruction map](architecture/system_contracts.md) ties the public journeys to their schema, state, authority, data and implementation contracts.
