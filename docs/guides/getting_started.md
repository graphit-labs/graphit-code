---
title: "Getting Started"
description: "Install the Rust executables, initialize a project and publish its native agent integration."
content-type: guide
audience: developers
updated: 2026-10-30
next: indexing.md
keywords:
  - setup
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
The download is about 558 MB; setup does not start it implicitly. See
[models and providers](ai_models.md) for selection and execution settings.
The daemon observes Global model-slot changes and refreshes their selection;
already admitted work retains its capture. Changing the captured completion
`ai.cli` binding instead requires daemon reattachment/restart.

## Initialize and maintain a project

The directory must already exist. Git is optional; a directory outside Git uses
its explicit version selection or the maintained `main` default.

```sh
cd your-project
graphit setup --agent kiro-v3 --project-dir .
```

Setup persists the native agent binding in the private installation configuration,
creates the current project identity through the SDK, and asks the authenticated
daemon to maintain the checkout. The daemon starts through its existing native
manager, registering the service if necessary; startup at login is an explicit
option. Setup reuses an existing project ID and metadata on subsequent runs.
It does not turn the installation directory into a project.

Version `.graphit/project.json`: it contains the stable `id`, root metadata
(`name`, optional `description` and `cluster`, `ephemeral`), ordinary `settings`
and optional root `githooks`. Subsequent setup/config/sync operations preserve
the other sections. Keep the ID unchanged across checkouts of the same project.
Installation-wide configuration stays in `~/.graphit/config.json`; credentials,
caches and generated state do not belong in the shared project document. See the
[project document](configuration.md#the-project-document) for fields and precedence.

On Unix, an existing `.<brand>/runtime` directory must be private (mode `700`);
setup rejects broader permissions rather than repairing them automatically. If
that directory belongs to your user, restrict only the directory itself and
repeat setup. For the default brand, run `chmod 700 .graphit/runtime` from the
project root; do not apply it recursively. Existing identity and store data stay
in place. See the [identity writer contract](../specs/project_identity.md#rust-local-identity-contract).

The JSON result identifies the installation, agent and project. Acknowledgement
means the checkout was admitted; indexing jobs can still be processing files.
No account profile, credentials or remote PROJECT permission is created by setup.
Configure named identities and remote access separately through
[authentication](authentication.md).

To configure only the installation, omit the project directory:

```sh
graphit setup --agent kiro-v3
```

Use `--name <name>` with `--project-dir` to choose a new project's initial name.
An existing identity keeps its metadata. Optional `--project-id <ULID>` also
requires `--project-dir`: a new identity uses that canonical, nonzero ID, an equal
existing ID is preserved, and a different ID is rejected before changing the
identity or installation agent setting. Omitting the option generates a new ID
when needed. This neither imports store data nor grants PROJECT permission.
Setup rejects `--project-version`: it initializes the actual checkout. Select a
retained version with `graphit projects maintain-checkout --project-id <PROJECT_ID> --project-dir . --version <version> --select`
when managing an existing checkout instead. A failure after configuration or identity publication
can leave those confirmed changes present; resolve the reported cause and rerun.
The SDK reuses the existing identity rather than creating a replacement.

## Publish the maintained Coordinator and hooks

For Kiro V3:

```sh
graphit coordinator install --binding kiro-v3
```

The release includes the authorial Coordinator files. The command captures them
and publishes the native agent, MCP configuration and PROJECT context hooks
through the existing authenticated SDKs. Use the returned `profile` value as the
agent selector in Kiro; do not assume a fixed filename or identifier. Follow the
host's required trust/reload steps in [agent hook activation](agent_hook_activation.md).
Start a new session after installation.

Generated hooks and MCP configuration call Graphit through PATH and use relative
PROJECT selectors. Ensure the native host can find the installed executable.
If this installation uses a custom root, its host environment must also contain
the branded `GLOBAL_DIR` variable; Codex forwards that variable by name using
`env_vars`. Do not replace generated commands with machine-specific checkout or
binary paths. See [bootstrap configuration](configuration.md#bootstrap-deployment-and-runtime-controls).

Select `kiro-v3` during setup and restart an already running daemon after changing
its binding. `graphit coordinator install --binding kiro-v3` publishes the current Markdown profile
and its inline context hooks. Start the native CLI with V3 explicitly selected;
Graphit's ACP consumers use `kiro-cli acp --agent-engine=v3`.
Other bindings can be selected for the
installation; their native publication and runtime qualification have their own
contracts in the [native binding guide](../development/native-host-bindings.md). Select each canonical --binding explicitly; Antigravity workspace MCP/hooks are supported without an undocumented customprofile MCP object.

A development binary without adjacent release assets can use
`graphit coordinator install --binding kiro-v3 --source-dir templates/agent-instructions/coordinator`
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
transport cannot run installation setup. A missing bundled Coordinator source
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
