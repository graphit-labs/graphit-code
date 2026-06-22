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
- **C Compiler** (only if building from source): `gcc` / `g++` (required for CGO-based SQLite extension FTS5 compilation).
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

**Linux / macOS:**
```bash
git clone https://github.com/graphit-labs/graphit-code.git
cd graphit-code
make install # installs to /usr/local/bin/ (sudo if not writable)
# make install PREFIX=$HOME/.local/bin # user-space alternative, no sudo
```

**Windows (MSYS2 / MinGW):**
```bash
git clone https://github.com/graphit-labs/graphit-code.git
cd graphit-code
make install-windows # installs to C:\Program Files\graphit\ (may need admin)
# make install-windows PREFIX_WIN='C:\Tools\graphit' # custom directory
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

### Step 2: Project Initialization
Navigate to your target codebase and link it to the Graphit Code system:
```bash
cd /path/to/your/project
graphit init --ide <antigravity|gemini|claude|cursor|kiro|codex|opencode>
```
The `--ide` flag automatically generates target rules, agent definitions, and rulesets tailored to your AI model coding assistants.
For example, it injects structured system instructions to enforce AST querying and memory preservation.

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
   Open your browser and navigate to `http://localhost:8080` to view the interactive 3D code graph.
