---
title: "Getting Started"
description: "Install the Rust executables, initialize a project and publish its native agent integration."
content-type: guide
audience: developers
updated: 2026-10-30
next: indexing.md
keywords:
  - init
  - installation
  - agents
prerequisites: []
related:
  - "docs/guides/cli_reference.md"
  - "docs/guides/configuration.md"
  - "docs/development/native-runtime-packaging.md"
references:
  - target: {type: task, id: tsk-0af7}
    relation: implements
---

# Getting Started

Use this guide to configure the Rust executables, initialize a project and publish
its maintained agent integration. The project workflow below uses the Linux
daemon. Native qualification for other platforms remains separate.

## Install the product

Linux or macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/graphit-labs/graphit-code/main/install.sh | sh
```

Choose a directory or release explicitly when needed (replace `RELEASE_TAG` with a published tag):

```sh
curl -fsSL https://raw.githubusercontent.com/graphit-labs/graphit-code/main/install.sh | sh -s -- --dir ~/.local/bin --version 'RELEASE_TAG'
```

Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/graphit-labs/graphit-code/main/install.ps1 | iex
```

The PowerShell installer accepts `-Dir` and `-Version` when downloaded and invoked
as a script; `GRAPHIT_INSTALL_DIR` also selects its destination. The Unix installer
supports Linux AMD64 and macOS ARM64; the Windows installer selects AMD64.
Installers verify the archive checksum and keep `graphit`, `graphit-model-server`
and `graphit-runtime` together. Add the reported directory to PATH. Manual
extraction must preserve that layout, including licenses and the bundled agent
sources, plus the app-local DLLs beside the Windows executables. Redistributable
native libraries are packaged; clients do not need Homebrew, OpenSSL, OpenBLAS
or a separate Visual C++ Redistributable installation. The supported operating
system libraries and an optional GPU driver remain host requirements. Check the
package's macOS runtime manifest for its actual minimum OS version. See
[native packaging](../development/native-runtime-packaging.md).

## Configure local embeddings explicitly

From any directory, install the pinned EmbeddingGemma2 model when local semantic
search is needed:

```sh
graphit models install embeddinggemma2
```

This downloads and verifies the pinned BF16 GGUF model. It adds the default
embedding selection when none exists, preserving an existing selection or an
explicit opt-out (`slots: []`). Use explicit model selection to enable embedding
after opting out.
The download is about 558 MB. Model installation is explicit. See
[models and providers](ai_models.md) for selection and execution settings.
The daemon observes Global model-slot changes and refreshes their selection;
already admitted work retains its capture. Changing the captured completion
`ai.cli` binding instead requires daemon reattachment/restart.

## Start working in a project

From a new project directory or a clone, run:

```sh
graphit init
```

Init discovers the checkout from the current directory. Supply `--project-dir`
to initialize a specific existing directory. Git is optional. Init creates the
project identity on the first run and preserves the identity and settings of
an existing project. It configures the installation, starts the daemon as needed,
registers applicable Git hooks and synchronizes the global and project
integrations. Background indexing can continue after the command completes.

Use `--agent <binding>` to select a host explicitly. Otherwise Init uses effective
project and installation settings and defaults to Codex on a fresh installation.
It saves an installation default on first use. Use `--name` and `--project-id`
to supply the initial project identity. A conflicting ID fails before the agent
configuration changes. Init operates on the actual checkout.

Version `.graphit/project.json` to share the stable project ID, settings and
Git hook configuration with other checkouts. Personal Memory and credentials
belong to the installation. On Unix, an existing `.graphit/runtime` directory
must be private (mode `700`). See the [configuration guide](configuration.md)
and [authentication guide](authentication.md).

Matching Coordinator, role and skill files brought in by a clone can be reused.
The result reports confirmed effects when a later step fails. Resolve the
reported cause and run Init again.

Follow the host's [trust and reload steps](agent_hook_activation.md) and open
a new session. For Kiro, use the profile reported by sync as the agent selector.

Generated hooks and MCP configuration call Graphit through PATH and use relative
PROJECT selectors. Ensure the native host can find the installed executable.
If this installation uses a custom root, its host environment must also contain
the branded `GLOBAL_DIR` variable; Codex forwards that variable by name using
`env_vars`. Do not replace generated commands with machine-specific checkout or
binary paths. See [bootstrap configuration](configuration.md#bootstrap-deployment-and-runtime-controls).

Run `graphit init --agent kiro-v3` to publish the Kiro integration. Project sync publishes the current Markdown profile and inline context hooks. Start the native CLI with V3 explicitly selected;
Graphit's ACP consumers use `kiro-cli acp --agent-engine=v3`.
Other bindings can be selected for the
installation; their native publication and runtime qualification have their own
contracts in the [native binding guide](../development/native-host-bindings.md). Select each canonical --binding explicitly; Antigravity workspace MCP/hooks are supported without an undocumented customprofile MCP object.

## Remove the project integration or installation

```sh
graphit remove --agent codex
graphit remove
graphit uninstall
```

Remove discovers the current project by default and accepts `--project-dir`.
Use `--agent` to withdraw that agent's owned integration while preserving the
project identity and shared Git hooks. Without an agent, Remove also retires
the checkout and deletes the Graphit project document. Retained datasets and
other checkouts can still be used independently.

Uninstall removes the user's service, managed integrations, and private data
recorded as belonging to the installation. It preserves project documents and
reports directories without an ownership record. A release package used by
multiple installations stays available until its last installation is removed.
On Windows, release-file cleanup runs after the executable exits. A binary
managed by another package manager or built from source is reported as
externally managed.

A development binary without adjacent release assets can use
`graphit init --coordinator-source templates/agent-instructions/coordinator`
from this repository. This override still undergoes the same product-content,
publication and authorization checks.

## Verify and operate

```sh
graphit daemon status
```

Status succeeds only for an observed running service. In a fresh agent session,
list the available Graphit MCP tools, read a known source file, and query the
project's actual AST, Knowledge and Memory data. A source read can use the disk
fallback; it alone does not prove that an index was built. Hooks deliver context
and MCP configuration exposes tools; host trust and tool permissions are separate.

The daemon maintains the admitted project versions and processes changes
incrementally. Configuration and source exclusions are described in
[configuration](configuration.md) and [ignore files](ignore_files.md).
For the authenticated browser, configure trusted Broker providers and the browser
PROJECT endpoint; see [container deployment](container.md) for the same Source
listener and explicit public-origin settings. There is no separate anonymous
browser startup step in this journey.

```sh
graphit daemon restart
graphit daemon stop
```

Stop reaches the retained owners and removes the daemon's own discovery and
runtime key. Project identities, configuration and persistent data remain.
Use `graphit daemon install --login` only when startup at user login is wanted;
`graphit daemon login disable` turns that registration off.

If startup fails, inspect the reported cause and the daemon's native manager logs.
Check that the private installation is accessible to the current user, the selected
binding is configured and model paths exist when enabled. A live managed-role
transport requires an initialized installation. A missing bundled Coordinator source
means the release layout is incomplete; restore `graphit-runtime` or use an explicit
verified development source directory.

## Build from source

Use the toolchain pinned in `rust-toolchain.toml`, Node.js 22, Make, CMake and a
C++20 toolchain with `<format>` support (GCC/libstdc++ 13 or newer on Linux).
The Make targets prepare Ladybug directly from the official Cargo dependency
using `cargo xtask cargo`; no separate native SDK is needed. See
[native packaging](../development/native-runtime-packaging.md) for build
prerequisites, consumed native assets and licenses.

```sh
npm --prefix ui ci
make ui
make build
```

The executables are produced by Cargo. `make release PLATFORM=linux-amd64` creates
the complete distribution, including the authorial Coordinator files. Use a
focused `make test TEST=<registered-suite>` for the affected suite during
implementation; see [the Makefile](../../Makefile) for current commands.
