---
title: "Private Deployment and Brand Customization Guide"
description: "A complete guide on deploying Graphit Code in 100% private, self-hosted environments with custom branding and enterprise collaboration configurations."
content-type: guide
audience: developers, operators
keywords:
  - private
  - white-label
  - brand
  - self-hosted
  - ldflags
  - registry
prerequisites:
  - "docs/guides/getting_started.md"
related:
  - "docs/specs/hub_collaboration.md"
  - "docs/specs/cluster_microservices.md"
---

# Private Deployment & Brand Customization

Graphit Code is designed from the ground up for maximum data privacy and flexibility. It can be completely self-hosted, operates 100% locally by default, and can be customized with your company's own brand (white-labeling) to create private collaboration ecosystems for IT teams.

---

## 🔒 100% Private Data Guarantee

Unlike traditional AI developer assistants that rely on cloud APIs and external databases, Graphit Code operates locally first:

1. **Local Graph Database:** Syntactic analysis is executed locally using Tree-sitter and stored inside an embedded **LadybugDB** instance on the developer's machine.
2. **Local Embeddings:** Semantic indexing utilizes a local ONNX runtime executing the `CodeRankEmbed-137M` model. No code leaves the developer's machine to compute vector embeddings.
3. **Git-Backed Hubs & Memory:** Memories and registries are stored as standard Git repositories. During the interactive `graphit setup` process, you can point them to your organization's private self-hosted GitLab, Bitbucket, or GitHub Enterprise instances to create a secure, shared collaborative workspace.
4. **Secure Local Tunnels:** If cloud-based LLM agents need access to your local context, you can set up secure, encrypted reverse tunnels (e.g., ngrok) bound to local TCP port listeners.

---

## 🎨 Custom Branding & White-Labeling

You can compile a customized, branded binary of Graphit Code to present a unified tool to your development teams.

The compilation process binds branding configuration values into the executable using Go linker flags (`-ldflags`).

### Customization Parameters

The following variables in `internal/brand/brand.go` can be overridden at compile time:

| Flag / Variable | Default Value | Description |
|---|---|---|
| `Brand` | `graphit` | Short identifier used for directories, file prefixes, and rules (e.g., `.mybrand/`, `mybrand.lock.json`). |
| `DisplayName` | `Graphit Code: AI Harness...` | The formal product name displayed in CLI help and user interface headers. |
| `GitHubRepo` | `graphit-labs/graphit-code` | Default repository used for update checks and issues. |
| `DefaultHubRepoURL` | `git@github.com:graphit...` | Default private Git repository used to synchronize team skills and rules. |
| `DefaultMemoryRepoURL` | `""` | Default private Git repository used to synchronize project memories. |

### Compilation Example

To build your custom binary, inject these variables into the compiler:

```bash
# Variables for customization
BRAND="devkit"
DISPLAY_NAME="Enterprise DevKit AI Harness"
HUB_URL="git@github.com:mycompany/devkit-hub.git"
MODULE="github.com/graphit-labs/graphit-code"

# Compile CLI core
go build -tags "fts5" -ldflags \
  "-X '${MODULE}/internal/brand.Brand=${BRAND}' \
   -X '${MODULE}/internal/brand.DisplayName=${DISPLAY_NAME}' \
   -X '${MODULE}/internal/brand.DefaultHubRepoURL=${HUB_URL}'" \
  -o .build/devkit-linux-amd64 ./cmd/launcher
```

Once built, the custom binary (e.g., `devkit`) will automatically:
- Create and read configuration directories under `~/.devkit/` and `.devkit/`.
- Look for configuration files named `devkit.lock.json`.
- Export MCP tools prefixed with `devkit_` (e.g., `devkit_ast_query`).
- Use the configured private repository as the default hub.

---

## 🤝 Setting up Private Collaboration Ecosystems

By deploying branded binaries across your engineering teams, you establish a secure, shared knowledge loop:

### 1. Centralized Rule Staging
Define company-wide coding guidelines or project-specific rules in your private Hub repository (e.g., `devkit-hub`). When developers run `devkit sync` or `devkit init`, the CLI pulls these rules and injects them directly into their IDE profiles (e.g., `.cursorrules`, `.claudecoderc`).

### 2. Standardized Team Skills
Codify complex developer workflows (e.g., k8s debugging, internal API structures) into custom agent skills under the `skills/` directory of your Hub repository. AI agents operating in individual developer workspaces can dynamically discover and run these skills.

### 3. Local Cluster Discovery
Use the local daemon cluster discovery to allow agents to discover other projects on the same machine. This enables agents to query sibling microservice structures locally to generate integration code without exposing private endpoints.

### 4. Collaborative Private IT Ecosystems (Git-Backed Setup)

Instead of relying on centralized databases or cloud-hosted synchronization services, Graphit Code achieves team collaboration by leveraging **standard Git repositories as backend sync stores**. This means your IT team's memories, coding rules, and custom developer skills are tracked, versioned, and shared securely using your existing Git infrastructure.

#### Configuration during CLI Setup

When developers run the interactive setup command:

```bash
graphit setup
```

The CLI prompts for key collaboration configurations:

```text
? Enter the Git URL for the Shared Hub Registry (e.g., git@github.com:company/graphit-hub.git):
? Enter the Git URL for the Shared Memory Repository (e.g., git@github.com:company/graphit-memory.git):
```

Once configured:
1. **Standard Git Authentication:** The CLI uses the developer's local SSH keys or Git credentials (e.g., HTTPS access tokens) to interact with the remote repository. No new credentials or keys are managed by Graphit Code, maintaining strict security boundaries and leveraging existing SSO/access controls.
2. **Push/Pull Sync Loop:** Staged memories, custom developer skills, and global coding rules are updated on the developer's machine and synced with the remote repository during synchronization (`graphit sync`), ensuring the entire team stays in a progressive, collaborative knowledge loop.

---

## 🔑 Keyless AI Harness (Zero API Key Setup)

One of the most powerful architectural features of Graphit Code is its ability to enable agentic tasks **without requiring separate LLM API access keys**. 

Traditionally, developer tools require configuring API keys (e.g., OpenAI, Anthropic, Gemini) or setting up complex proxy relays in every developer's environment, exposing organizations to credential leaks and separate billing management. Graphit Code completely bypasses this requirement.

### How it Works

Graphit Code operates as a Model Context Protocol (MCP) server over standard input/output (`stdio`) or HTTP. It does not initiate connections to external LLM providers directly to serve tools; instead, the **calling agent** (e.g., Claude Code running in a terminal, or Gemini Code Assist in the IDE) is the one that is authenticated and communicates with the LLM backend.

When an agent interacts with your workspace:
1. The developer's IDE or CLI agent (which is already authenticated to its respective LLM backend) starts the Graphit Code process in the background.
2. The agent queries Graphit Code's MCP tools (such as AST querying, memory lookup, and wiki explorer) via standard stdio JSON-RPC.
3. Graphit Code responds locally with deterministic AST code graphs, memory context, and wiki entries.
4. The calling agent uses this localized context to formulate its final responses and code edits using its **own active accounts, quotas, and capabilities**.

### Key Benefits

- **Zero API Keys Configured:** Developers do not need to configure, store, or manage any API keys inside Graphit Code's global or local profiles.
- **Quota & Cost Reusability:** The harness automatically reuses the existing subscriptions, active quotas, and capabilities of the developer's IDE and CLI agents (e.g., Claude Code, Cursor, Gemini Code Assist).
- **Enterprise Controls:** Leverages the enterprise security controls, Single Sign-On (SSO), and logging mechanisms already set up for developer IDE/CLI tools.
