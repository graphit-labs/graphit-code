---
title: "Graphit Code Documentation"
description: "Unified documentation hub for Graphit Code, containing onboarding guides, developer references, and architecture specifications."
content-type: guide
audience: developers
keywords:
  - documentation
  - index
  - guides
  - specifications
---

# Graphit Code Documentation Hub

Welcome to the official documentation for Graphit Code.
Graphit Code is the enterprise collaboration hub for AI-guided software development.
It synchronizes codebase structure (AST), persistent memory, and shared team specifications to keep local and remote AI agents aligned.

This documentation is divided into three key areas:
1. **User and Operator Guides** — Instructions on installing, setting up, and using Graphit Code.
2. **Architecture Overview** — High-level designs explaining how the launcher, CLI, and visual layers interconnect.
3. **Module Specifications** — Technical deep-dives into each subsystem.

---

## 📖 User and Operator Guides

Follow these manuals to get started and master Graphit Code:

- **[Getting Started](guides/getting_started.md)**: Install, configure development environments, and initialize Graphit Code.
- **[CLI Command Reference](guides/cli_reference.md)**: Detailed reference sheet of all commands and options.
- **[User Manual](guides/user_manual.md)**: Operator manual for writing wikis, managing memories, using the registry hub, and navigating the 3D dashboard.
- **[Private Branding & Customization](guides/private_brand_customization.md)**: Build custom branded binaries, configure private hubs, and deploy 100% private developer harnesses.
  - 🏆 **[Private Team Collaboration](guides/private_brand_customization.md#-setting-up-private-collaboration-ecosystems)**: Configure private Git repositories as your team's **Hub** (centralized artifact registry) and **Memory** (shared team intelligence) — enabling a fully private, self-hosted collaboration ecosystem where knowledge, corrections, and standards compound across every developer.

---

## 🏗️ System Architecture

Understand how the Graphit Code system is structured:

- **[System Architecture Overview](architecture/architecture_overview.md)**: Explanation of binary wrappers, decoupling, persistence, and IDE integrations.

---

## 🔧 Subsystem Specifications

Explore the engineering specifications of each internal package:

- **[AST Module Specification](specs/ast_module.md)**: LadybugDB graph schema, Cypher translation, language syntax trees, and hybrid FTS/semantic search.
- **[Wiki Module Specification](specs/wiki_module.md)**: Obsidian wiki, BM25 indices, Louvain community detection, and fuzzy reference resolution.
- **[Memory Module Specification](specs/memory_module.md)**: Git-backed project and user memory wikis, cycles, and memory rules.
- **[Hub Collaboration Specification](specs/hub_collaboration.md)**: Registry managers, lockfile metadata tracking, and artifact publishing.
- **[Daemon Module Specification](specs/daemon_module.md)**: Background daemon, shared embedding model client, OS schedulers, and replacement spawns.
- **[Dream Module Specification](specs/dream_module.md)**: Autonomous skill generation, conversation mining, skill effectiveness evaluation, and knowledge extraction.
- **[Improvements Module Specification](specs/improvements_module.md)**: Resolution order, engineering analysis methodology (Clean Code, Security, Observability), and reflection phase.
- **[UI Dashboard Specification](specs/ui_dashboard.md)**: Vite-React application, Force-Directed 3D canvas, state stores, and uiserver handlers.
- **[AI Engine Specification](specs/ai_engine.md)**: Model manager, embedding backends (local, proxy, lazy), and Cypher AI helpers.
- **[Cluster Discovery Specification](specs/cluster_microservices.md)**: Sibling project registration, multi-service navigation, and cross-project query delegations.
