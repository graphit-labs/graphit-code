# Graphit Code Documentation

Graphit connects four kinds of project context: code structure, maintained documentation, durable memory, and reusable ecosystem artifacts. This hub routes you to the current guidance for each job without mixing it with historical implementation records.

![Graphit Knowledge Explorer showing this project's architecture](site/assets/observatory-knowledge-explorer.jpg)

## Choose your path

| I want to… | Start here | Continue with |
|---|---|---|
| Install Graphit and initialize a repository | [Getting Started](guides/getting_started.md) | [CLI Reference](guides/cli_reference.md) |
| Use Graphit day to day | [User Manual](guides/user_manual.md) | [Troubleshooting](guides/troubleshooting.md) |
| Configure an agent or MCP client | [MCP Tools Reference](guides/mcp_tools_reference.md) | [Retrieval Architecture](guides/retrieval_architecture.md) |
| Understand system boundaries | [Architecture Overview](architecture/architecture_overview.md) | [Storage Layout](architecture/storage_layout.md) |
| Operate shared storage or networked UI | [S3 and UI Network](guides/s3-and-ui-network.md) | [Hub S3 Object Layout](specs/hub-s3-object-layout.md) |
| Customize a private distribution | [Private Brand Customization](guides/private_brand_customization.md) | [Configuration Specification](specs/config_module.md) |
| Contribute to the project | [Contributing](../CONTRIBUTING.md) | [Repository README](../README.md) |

## Guides

Guides explain workflows from a user or operator perspective.

- [Getting Started](guides/getting_started.md) — installation, setup, project initialization, first sync, and the Observatory.
- [User Manual](guides/user_manual.md) — everyday AST, knowledge, memory, Hub, daemon, Dream, and ecosystem workflows.
- [CLI Command Reference](guides/cli_reference.md) — commands, flags, aliases, and expected effects.
- [MCP Tools Reference](guides/mcp_tools_reference.md) — agent-facing tool contracts and required parameters.
- [Retrieval Architecture](guides/retrieval_architecture.md) — when to search, read source, traverse a graph, or use live search.
- [Ignore Files](guides/ignore_files.md) — source and documentation exclusion behavior.
- [S3 Credentials and UI Network](guides/s3-and-ui-network.md) — optional remote storage, credentials, binding, CORS, and security boundaries.
- [Private Brand Customization](guides/private_brand_customization.md) — branded binaries and private collaboration environments.
- [Troubleshooting](guides/troubleshooting.md) — common operational failures and diagnostics.

## Architecture

- [System Architecture Overview](architecture/architecture_overview.md) — launcher, core engine, modules, daemon, UI, Hub, and IDE adapters.
- [Storage Layout](architecture/storage_layout.md) — global stores, project records, contexts, caches, and published data.

## Module specifications

Specifications describe current module behavior and technical contracts.

- [AST Module](specs/ast_module.md)
- [Wiki Module](specs/wiki_module.md)
- [Memory Module](specs/memory_module.md)
- [Hub Collaboration](specs/hub_collaboration.md)
- [Hub S3 Object Layout](specs/hub-s3-object-layout.md)
- [Configuration Module](specs/config_module.md)
- [Daemon Module](specs/daemon_module.md)
- [Dream Module](specs/dream_module.md)
- [Task Backlog](specs/backlog.md)
- [UI Dashboard](specs/ui_dashboard.md)
- [AI Engine](specs/ai_engine.md)
- [Cluster Discovery](specs/cluster_microservices.md)

## Historical records

The following areas are intentionally preserved as chronological evidence:

- `tasks/` — implementation logs, investigations, measurements, and unfinished debt.
- `changelogs/` — feature and migration history.
- `decisions/` — accepted architectural decisions and their trade-offs.
- `reports/` and `testing/` — point-in-time validation evidence.

Historical records may describe superseded behavior. For current usage, prefer guides, architecture pages, and module specifications; follow links into history when you need rationale or provenance.

## Documentation conventions

- Maintained documentation and the root README are written in English.
- User workflows belong in `guides/`; system boundaries in `architecture/`; module contracts in `specs/`.
- Every implementation task opens and maintains a task log under `tasks/`.
- Commands and file paths use code formatting; security limitations are stated next to the relevant configuration.
- Screenshots should use the current Graphite Observatory UI and a first-party Graphit Code project context.

## Keep the wiki current

The daemon watches the documentation tree and rebuilds the local knowledge wiki after edits. Use `graphit sync` when you need an explicit all-system checkpoint across AST, knowledge, memory, and Hub.
