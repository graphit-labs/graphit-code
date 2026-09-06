# Getting Started

This guide takes a project directory from no Graphit installation to a working agent integration, current local indexes, and the Graphit Observatory UI. Git is optional.

## What you will have

After the first run:

- the `graphit` launcher and machine-wide runtime are installed;
- the local embedding model is available to search modules;
- the project directory is registered and configured for your selected IDE;
- AST, knowledge, and memory indexes reflect the current project;
- the daemon can keep those indexes current;
- the UI can inspect the same project context exposed to agents.

## 1. Install the launcher

### Linux or macOS

```bash
curl -fsSL https://raw.githubusercontent.com/graphit-labs/graphit-code/main/install.sh | sh
```

Use a custom installation directory when needed:

```bash
curl -fsSL https://raw.githubusercontent.com/graphit-labs/graphit-code/main/install.sh | sh -s -- --dir ~/.local/bin
```

Pin a release instead of taking the newest, which is what a reproducible install or a container
image needs. The archive's SHA-256 is verified either way:

```bash
curl -fsSL https://raw.githubusercontent.com/graphit-labs/graphit-code/main/install.sh | sh -s -- --version v0.1.26
```

### Windows PowerShell

```powershell
irm https://raw.githubusercontent.com/graphit-labs/graphit-code/main/install.ps1 | iex
```

The Windows installer defaults to `%LOCALAPPDATA%\Programs\graphit`. To choose another destination,
download the script and pass `-Dir`, or set `GRAPHIT_INSTALL_DIR` before invoking it. The parameter
wins over the environment variable. The current PowerShell installer selects the latest release;
for a pinned Windows install, download and verify the named release archive directly.

The release installers detect the operating system and architecture, download the latest archive, verify its SHA-256 checksum, and install in a user-owned location. Add the reported directory to `PATH` if the installer cannot do so automatically.

## 2. Prepare the machine

```bash
graphit setup
```

Setup prepares the global Graphit directory and downloads the local embedding model once per machine. Optional S3-compatible Hub configuration can be added during setup or later; it is not required for local AST, knowledge, memory, or UI use.

If credentials are configured explicitly, both access and secret keys must be present. Leaving them unset lets AWS-compatible clients use the standard provider chain. See [S3 Credentials and UI Network Configuration](s3-and-ui-network.md).

## 3. Initialize a project

Run initialization from the project root Graphit should understand:

```bash
cd your-project
graphit init --ide codex
```

Replace `codex` with the IDE or coding-agent adapter you use. Run `graphit init --help` to see supported identifiers and options.

Initialization registers the project and installs the selected adapter's hooks, MCP configuration, skills, commands, and agents. Graphit mandates and installed Hub rules are delivered dynamically by the hooks; Graphit does not manage `AGENTS.md`, `CLAUDE.md`, or IDE rule files for that purpose. Review generated changes before sharing or committing them.

The project's immutable ULID may already exist: the first earlier command that needed durable
project state created a minimal `graphit.lock.json`. Initialization reuses that ULID and completes
adapter and baseline setup. The friendly project name may later change without re-keying any local
or remote store. See [Project Identity](../specs/project_identity.md).

Each developer must complete any trust or activation step required by their agent on their own machine. Follow [Activate Graphit Hooks in Each Agent](agent_hook_activation.md) for exact per-adapter instructions.

## 4. Build the first indexes

```bash
graphit sync
```

The initial sync builds the code graph, documentation wiki, and memory indexes. Subsequent runs reuse content hashes and update changed inputs.

Graphit reads maintained project documentation from `docs/` and includes the root `README.md` in the knowledge wiki. Source exclusions and documentation exclusions are separate; see [Ignore Files](ignore_files.md).

## 5. Open the Observatory

```bash
graphit ui
```

Open the displayed loopback URL. The workspace selector should identify the current repository before you interpret explorer data.

![Graphit AST Explorer analyzing graphit-code](../site/assets/observatory-ast-explorer.jpg)

The UI is not an authentication layer. Keep the default loopback binding for local use; read [S3 Credentials and UI Network Configuration](s3-and-ui-network.md) before making it reachable over a network.

To serve a whole team instead of one machine, run it as a container: the daemon as PID 1, publishing an MCP endpoint that any AI agent can connect to plus this same UI. See [Run as a Server in a Container](container.md).

## 6. Verify the agent tools

First complete the adapter-specific trust and hook check in [Activate Graphit Hooks in Each Agent](agent_hook_activation.md). Then start a fresh session in the configured IDE and ask the agent to:

1. list available Graphit MCP tools;
2. search the project AST for a known symbol;
3. read one relevant knowledge page;
4. search project memory for a repository convention.

Search tools return ranked names or titles. The agent should read the selected source or wiki page, and use graph traversals for relationship questions.

## Daily operation

The daemon watches registered projects and schedules incremental maintenance:

```bash
graphit daemon
```

Ordinary CLI and MCP startup ensures it automatically unless `modules.daemon=false`. Use
[Daemon Operations and Monitoring](daemon_operations.md) for scheduler installation, every watched
signal and maintenance loop, project parking, logs, and recovery.

Use an explicit sync after bulk external changes such as a checkout, pull, rebase, or restore, or whenever you need a verified all-system checkpoint.

Useful next commands:

```bash
graphit ast schema
graphit memory list
graphit knowledge search "architecture"
graphit ui
```

Confirm exact syntax for your installed version with `graphit --help` and the [CLI Command Reference](cli_reference.md).
Choose completion CLIs, embedding models, dimensions, and remote provider boundaries with
[AI Models, Providers, and Agent CLIs](ai_models.md).

## Build from source

Source builds require:

- Go 1.26.6 or newer;
- Node.js 22 or newer;
- Make;
- a C/C++ toolchain for native bindings.

The normal build downloads and verifies an immutable native dependency bundle. Rust is required
only in the companion `graphit-code-libs` repository when rebuilding the pinned LanceDB bridge.

```bash
git clone https://github.com/graphit-labs/graphit-code.git
cd graphit-code
make install
graphit setup
```

Platform-specific Make targets and development checks are documented in the repository [Contributing Guide](../../CONTRIBUTING.md).

## Troubleshooting the first run

### `graphit` is not found

Add the installer destination to `PATH`, open a new shell, and run `graphit --version`.

### The UI shows another project

Use the workspace selector to activate the intended repository and verify its name before trusting explorer counts. Run `graphit ui` from the target project directory.

### Search works but semantic results are empty

Check that `graphit setup` completed the model download. Keyword search remains available while semantic vectors are being prepared.

### The index appears stale

Run `graphit sync` from the repository root. If the problem persists, inspect daemon status and continue with [Troubleshooting](troubleshooting.md).

## Next steps

- [User Manual](user_manual.md) for daily workflows.
- [Capability and Surface Matrix](capability_matrix.md) for what is available in CLI, MCP, and UI.
- [Configuration Reference](configuration.md) for every key, default, module switch, and environment override.
- [Filesystem, State, and Watchers](filesystem_contract.md) for files, paths, generated state, and change detection.
- [CLI Command Reference](cli_reference.md) for command syntax.
- [MCP Tools Reference](mcp_tools_reference.md) for agent-facing contracts.
- [Agent Hook Activation](agent_hook_activation.md) for trust, reload, and verification steps per adapter.
- [Architecture Overview](../architecture/architecture_overview.md) for system boundaries.
- [Storage Layout](../architecture/storage_layout.md) for local and shared data locations.
