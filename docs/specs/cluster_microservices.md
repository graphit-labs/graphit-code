---
title: "Cluster Discovery Specification"
description: "Project discovery metadata, current inventory interfaces and the boundary of cluster discovery support."
content-type: reference
audience: developers
keywords:
  - cluster
  - microservices
  - siblings
  - active projects
  - cross-project query
prerequisites:
  - "docs/architecture/architecture_overview.md"
related:
  - "docs/specs/ast_module.md"
  - "docs/specs/hub_collaboration.md"
---

# Project discovery and cluster metadata

This page separates the current Rust inventory from historical cluster command designs. A stable
PROJECT identity can carry metadata such as labels without exposing a public label mutation or
automatic sibling-discovery API. Metadata support alone does not prove an executable workflow.

## Registered projects and current interfaces

The private `<global-dir>/project-inventory.json` records stable project IDs, retained versions
and actual maintained checkouts. Paths are Unicode strings. A checkout path locates a working tree;
it neither identifies the corpus nor authorizes another project. See [Project identity](project_identity.md)
and [Storage](../architecture/storage_layout.md).

The supported CLI is `graphit projects maintain-checkout` / `retire-checkout`, with the exact
arguments in the [CLI reference](../reference/cli-options.md#graphit-projects-maintain-checkout).
Maintaining a checkout preserves sibling jobs and publishes the actual inventory. Retirement stops
that checkout's jobs while retaining its versions and records. It does not erase the corpus.
Use `graphit projects list` or Root MCP `graphit_projects_list` to find admitted
local projects, including by exact label key and optional value. The inventory
also returns each project's description and labels.

## Local project labels

`ProjectMetadata.cluster` stores label keys and their values in
`.graphit/project.json`. Use `graphit projects label-set` and `label-unset`
or Root MCP `graphit_projects_labels_set` and `graphit_projects_labels_unset`
to edit the metadata of an exact active checkout under Native project
administrative authority. See [Local project discovery](../guides/cli_reference.md#local-project-discovery)
for examples and the [inventory contract](rust_project_inventory.md) for
publication and partial effects.

Hub catalog discovery uses its explicit provider/producer/type/version contract and permissions.
It lists remote artifacts, not arbitrary local checkouts or projects accessible through the host's
filesystem. [Hub collaboration](hub_collaboration.md) explains those operations.

## Cross-project reads after legitimate selection

When a host or supported discovery interface supplies a real admitted project directory, use that
exact `project_dir` only in live module calls and read the target's enabled module instructions.
AST, Knowledge, Memory and Task all recheck their own project/version authority. A visible directory,
catalog hit, label or stable ID grants no access by itself. Missing discovery remains a reported
capability gap rather than a guessed sibling path.

Reuse sufficient source evidence and recall only the Memory/Task history needed for the question.
The coordinating Task stays in the owning project; read access does not authorize writes elsewhere.
Persist logical project ID/version and repository-relative evidence, never one host's checkout root.
Imported Hub contexts without checkouts instead use their exact artifact source selector.

See the [MCP catalog](../guides/mcp_tools_reference.md), [retrieval architecture](../guides/retrieval_architecture.md)
and [capability matrix](../guides/capability_matrix.md) for the actual current surfaces.
