---
title: "Getting Started"
description: "Learn how to install, configure, and initialize Graphit Code in your development environment."
content-type: tutorial
audience: developers
keywords:
  - install
  - setup
  - init
  - get started
  - config
prerequisites:
  - "docs/README.md"
related:
  - "docs/guides/cli_reference.md"
  - "docs/guides/user_manual.md"
---

# Getting Started With Graphit Code

This guide walks you through installing, setting up, and initializing Graphit Code in your environment.
Whether you are using pre-compiled binaries or building from source, follow these steps to configure your enterprise AI-guided coding experience.

---

## 📋 Prerequisites

Before installing, ensure your environment meets the minimum requirements:

- **Operating System**: Linux, macOS (Apple Silicon or Intel), or Windows (10/11).
- **Go compiler** (only if building from source): Version 1.23 or newer.
- **Node.js** (only if building from source): Version 22 or newer.
- **C Compiler** (only if building from source): `gcc` / `g++`, for the CGO bindings to the graph and search engines.
- **Rust** (only if building from source): the LanceDB search engine is built from source for the host — `make fetch-lancedb`. It cannot be cross-compiled, which is why a release builds on one runner per platform.
- **Git CLI**: Required for persisting memory and knowledge registries.

---

## 💾 Installation Options

Choose one of the following methods to install the `graphit` CLI:

### Option 1: One-liner Install (Recommended)

**Linux / macOS:**
```bash
curl -fsSL https://raw.githubusercontent.com/graphit-labs/graphit-code/main/install.sh | bash
# Custom directory:
# curl -fsSL https://raw.githubusercontent.com/graphit-labs/graphit-code/main/install.sh | bash -s -- --dir ~/.local/bin
```

**Windows (PowerShell):**
```powershell
irm https://raw.githubusercontent.com/graphit-labs/graphit-code/main/install.ps1 | iex
# Custom directory:
# iex "& { $(irm https://raw.githubusercontent.com/graphit-labs/graphit-code/main/install.ps1) } -Dir '$env:LOCALAPPDATA\Programs\graphit'"
```

The installer detects your OS/arch, verifies SHA-256, and installs to `~/.local/bin/graphit` (Linux/macOS) or `%LOCALAPPDATA%\Programs\graphit\` (Windows). It will warn if the directory is not yet in your `PATH`.

### Option 2: Direct Download (Manual)

#### Linux (amd64)
```bash
curl -fsSL https://github.com/graphit-labs/graphit-code/releases/latest/download/graphit-linux-amd64.tar.gz | tar -xz
mkdir -p ~/.local/bin && mv graphit-linux-amd64 ~/.local/bin/graphit
```

#### macOS (Apple Silicon)
```bash
curl -fsSL https://github.com/graphit-labs/graphit-code/releases/latest/download/graphit-darwin-arm64.tar.gz | tar -xz
mkdir -p ~/.local/bin && mv graphit-darwin-arm64 ~/.local/bin/graphit
```

#### Windows (PowerShell)
```powershell
$InstallDir = "$env:LOCALAPPDATA\Programs\graphit"
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Invoke-WebRequest -Uri "https://github.com/graphit-labs/graphit-code/releases/latest/download/graphit-windows-amd64.tar.gz" -OutFile "$env:TEMP\graphit.tar.gz"
tar -xzf "$env:TEMP\graphit.tar.gz" -C "$env:TEMP"
Move-Item -Force "$env:TEMP\graphit-windows-amd64.exe" "$InstallDir\graphit.exe"
[System.Environment]::SetEnvironmentVariable("PATH", "$([System.Environment]::GetEnvironmentVariable('PATH','User'));$InstallDir", "User")
```

### Option 3: Build From Source

If you prefer to compile the application locally, clone the repository and compile the binaries:

**Linux:**
```bash
git clone https://github.com/graphit-labs/graphit-code.git
cd graphit-code
make install # installs to /usr/local/bin/ (sudo if not writable)
# user-space alternative, no sudo
# make install PREFIX=$HOME/.local/bin
```

**macOS:**
```bash
git clone https://github.com/graphit-labs/graphit-code.git
cd graphit-code
make install-darwin # installs to /usr/local/bin/ (sudo if not writable)
# user-space alternative, no sudo
# make install-darwin PREFIX=$HOME/.local/bin
```

**Windows (MSYS2 / MinGW):**
```bash
git clone https://github.com/graphit-labs/graphit-code.git
cd graphit-code
make install-windows # installs to C:\Program Files\graphit\ (may need admin)
# custom directory
# make install-windows PREFIX_WIN='C:\Tools\graphit'
```

The Makefile automates dependencies installation, compiles the React UI static bundle, and embeds assets into Go source files. The `PREFIX` variable (Linux/macOS) and `PREFIX_WIN` variable (Windows) control the install directory.

---

## 🚀 Initial Configuration

Once the CLI is installed, execute the configuration steps:

### Step 1: Global Setup
Initialize your global configuration, centralized knowledge hub, and memory storage directories:
```bash
graphit setup
```
This command configures files inside the global directory at `~/.graphit`.

When you provide an S3-compatible Hub bucket, setup asks for optional access and secret
keys. Enter both to save an explicit global credential pair, or leave either blank to
use the existing AWS provider chain (profiles, environment, container credentials, or
workload roles). The secret is hidden while typing and redacted from config output, but
it is plain text in the owner-only `~/.graphit/config.json`; provider-chain credentials
are preferred. Full behavior and reset instructions are in
[S3 Credentials and UI Network Configuration](s3-and-ui-network.md).

Its last step downloads the embedding model — around 132 MB, once per machine, into
`~/.graphit/models/coderankembed/`. On a terminal it shows a progress bar; piped to a
file it reports in tenths. The model is deliberately **not** built into the binary, so
this is where it arrives.

**A failed download fails the setup**, with a non-zero exit status. An installation
with no model cannot compute embeddings, so it cannot answer a semantic query, and
reporting success would hide that until some later search quietly came back on
keywords alone.

Everything you entered before that point is already saved, so recovery is just
re-running it once the network is sorted:

```bash
graphit setup
```

Behind a proxy, set `HTTP_PROXY` / `HTTPS_PROXY` first. On a machine with no route to
Hugging Face at all, place `model.onnx` and `tokenizer.json` in the cache directory by
hand and setup will find them and succeed — see
[Air-Gapped Deployments](private_brand_customization.md#-air-gapped-deployments).

### Step 2: Project Initialization
Navigate to your target codebase and link it to the Graphit Code system:
```bash
cd /path/to/your/project
graphit init --ide <antigravity|gemini|claude|cursor|kiro|codex|opencode>
```
The `--ide` flag automatically generates target rules, agent definitions, and rulesets tailored to your AI model coding assistants.
For example, it injects structured system instructions to enforce AST querying and memory preservation.

Initialization also maintains a marked block in `.gitignore`:

```gitignore
# --- GRAPHIT AUTOGENERATED IGNORER ---
**/.graphit/runtime/
**/.graphit/grammars/
# --- END GRAPHIT AUTOGENERATED IGNORER ---
```

Keep `.graphit/ast/queries/**` and `.graphit/rules/**` in version control when the team
shares those overrides. `.graphit/grammars/**` contains local parser binaries, while
generated exports, Dream reports, caches, logs, locks, and stamps live under
`.graphit/runtime/**`; both trees are intentionally ignored.

---

## 🔍 Verification

Verify that Graphit Code is fully active:

1. **Verify Daemon**: Ensure the background synchronizer is active:
   ```bash
   graphit daemon status
   ```
2. **Start Interactive Dashboard**: Launch the visual explorer interface:
   ```bash
   graphit ui
   ```
   The command opens the browser on the free port it selected. The listener binds to all
   IPv4 interfaces by default (`ui.host=0.0.0.0`), while browser CORS remains localhost-only
   until `ui.allowed_origins` is configured. The UI has no authentication; set
   `ui.host=127.0.0.1` for local-only use or protect remote access with a firewall, VPN, or
   authenticated reverse proxy. See the [network guide](s3-and-ui-network.md).
