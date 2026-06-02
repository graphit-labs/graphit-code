---
title: "Troubleshooting"
description: "Diagnose and resolve common issues with Graphit Code — daemon, AST indexing, AI/embedding, memory, hub, MCP, and configuration problems."
content-type: guide
audience: developers
keywords:
  - troubleshooting
  - errors
  - debug
  - daemon
  - fix
  - common issues
prerequisites:
  - "docs/guides/getting_started.md"
related:
  - "docs/guides/mcp_tools_reference.md"
  - "docs/guides/cli_reference.md"
  - "docs/guides/user_manual.md"
---

# Troubleshooting

This guide covers common issues you may encounter when using Graphit Code and how to resolve them. Issues are organized by module — jump to the relevant section for your problem.

---

## Table of Contents

- [Quick Diagnostics](#quick-diagnostics)
- [Installation & Setup](#installation--setup)
- [Configuration Issues](#configuration-issues)
- [Daemon Issues](#daemon-issues)
- [AST Indexing Issues](#ast-indexing-issues)
- [AI & Embedding Issues](#ai--embedding-issues)
- [Memory Issues](#memory-issues)
- [Knowledge Issues](#knowledge-issues)
- [Hub Issues](#hub-issues)
- [MCP Connection Issues](#mcp-connection-issues)
- [Dream Module Issues](#dream-module-issues)
- [Getting Help](#getting-help)

---

## Quick Diagnostics

Run these commands first to gather diagnostic information:

```bash
# Check Graphit version
graphit version

# Check daemon status (PID, uptime, scheduler, recent logs)
graphit daemon status

# Check if the project is initialized
ls -la .graphit-lock.json

# Check global configuration
graphit config list --global

# Check project configuration
graphit config list

# Verify git is available (required for memory/hub)
git --version

# Run a full sync to detect issues
graphit sync
```

> **Tip:** The `graphit_daemon_status` MCP tool returns structured JSON with the daemon PID, uptime, scheduler status, and the last 10 lines of the daemon log — very useful for automated diagnostics.

---

## Installation & Setup

### Binary not found after installation

**Symptoms:**
```
graphit: command not found
```

**Solutions:**
1. Ensure the binary is in your `PATH`:
   ```bash
   which graphit
   echo $PATH
   ```
2. If installed manually, move to a directory on your `PATH`:
   ```bash
   sudo mv graphit /usr/local/bin/
   ```
3. On macOS, you may need to approve the binary in System Preferences → Security & Privacy after the first run.

### Build from source fails with CGO errors

**Symptoms:**
```
cgo: C compiler "gcc" not found: exec: "gcc": executable file not found in $PATH
```

**Solutions:**
- **Linux:** Install `gcc` and `g++`:
  ```bash
  sudo apt-get install build-essential   # Debian/Ubuntu
  sudo dnf install gcc gcc-c++           # Fedora/RHEL
  ```
- **macOS:** Install Xcode Command Line Tools:
  ```bash
  xcode-select --install
  ```
- CGO is required for the FTS5 SQLite extension used by the AST graph database.

### Project not initialized

**Symptoms:**
```
project not initialized. Run init first
project not initialised
```

**Solutions:**
```bash
graphit init
```
This creates the `.graphit-lock.json` file and project identity (ULID). If you are using an IDE adapter, specify it:
```bash
graphit init --ide claude
```

---

## Configuration Issues

### Global config parse error

**Symptoms:**
```
parsing global config: ...
reading global config: ...
```

**Cause:** The global configuration file (`~/.graphit/config.json`) is corrupted or contains invalid JSON.

**Solutions:**
1. View the raw file:
   ```bash
   cat ~/.graphit/config.json
   ```
2. Validate JSON syntax (look for trailing commas, unquoted keys).
3. If the file is beyond repair, back it up and reset:
   ```bash
   mv ~/.graphit/config.json ~/.graphit/config.json.bak
   ```
   Graphit will recreate defaults on next use.

### Configuration key not taking effect

**Cause:** Graphit uses a cascading configuration priority system. Higher-priority sources override lower ones:

1. **Inline parameters** (tool call arguments) — highest priority
2. **Environment variables**
3. **Project config** (`.graphit-lock.json` → `config` section)
4. **Global config** (`~/.graphit/config.json`)
5. **Compiled defaults** — lowest priority

**Diagnosis:**
```bash
# Check effective project config
graphit config list

# Check global config
graphit config list --global

# Verify a specific key
graphit config get <key>
graphit config get <key> --global
```

### IDE adapter not detected

**Symptoms:** Rules are not installed for your IDE, or the wrong IDE format is used.

**Solutions:**
1. Set the IDE explicitly:
   ```bash
   graphit config set ide claude          # Project-level
   graphit config set ide cursor --global # Global default
   ```
2. Re-run sync to apply:
   ```bash
   graphit sync --ide claude
   ```

---

## Daemon Issues

### Daemon already running

**Symptoms:**
```
daemon already running (pid <N>, started <timestamp>)
```

**Cause:** Another daemon instance is already active. The daemon is a singleton — only one process runs globally.

**Solutions:**
1. Check the running daemon:
   ```bash
   graphit daemon status
   ```
2. Stop it if you need to restart:
   ```bash
   graphit daemon stop
   ```
3. If the daemon is stuck, force kill:
   ```bash
   graphit daemon stop  # Sends SIGTERM, then SIGKILL after 10s
   ```

### Stale PID file

**Symptoms:** Daemon reports as running but the process does not exist, or commands hang.

**Cause:** The daemon crashed without cleaning up its PID file.

**Solutions:**
1. Check if the process is actually alive:
   ```bash
   graphit daemon status
   # Look at the "running" field
   ```
2. The PID file is stored at `~/.graphit/daemon/daemon.pid`. If the process does not exist, the daemon tools will detect it and allow a new start.
3. Manual cleanup (last resort):
   ```bash
   rm ~/.graphit/daemon/daemon.pid
   ```

### Daemon crashes on startup

**Cause:** The daemon failed to write its log file or PID file.

**Solutions:**
1. Check directory permissions:
   ```bash
   ls -la ~/.graphit/daemon/
   ```
2. Check the daemon log for errors:
   ```bash
   cat ~/.graphit/daemon/daemon.log
   ```
3. Ensure disk space is available:
   ```bash
   df -h ~/.graphit
   ```

### Daemon keeps restarting (crash loop)

**Cause:** The daemon has a maximum restart limit of **10 restarts**. If a module crashes within 60 seconds of starting, it counts toward the limit. After 10 fast failures, the module is disabled.

**Solutions:**
1. Check the daemon log for the failing module:
   ```bash
   tail -50 ~/.graphit/daemon/daemon.log
   ```
2. Look for patterns like embedding model download failures or port conflicts.
3. Stop the daemon, fix the underlying issue, then restart:
   ```bash
   graphit daemon stop
   # Fix the issue...
   graphit sync  # This will auto-start the daemon
   ```

### OS scheduler not configured

**Symptoms:** The daemon does not autostart after system reboot.

**Diagnosis:**
```bash
graphit daemon status
# Check the "scheduler_status" field
```

**Solutions by OS:**
- **Linux (systemd):** The daemon registers a systemd user unit.
  ```bash
  systemctl --user status graphit-daemon
  systemctl --user enable graphit-daemon
  ```
- **macOS (launchd):** The daemon registers a Launch Agent.
  ```bash
  launchctl list | grep graphit
  ```
- **Windows (schtasks):** The daemon registers a scheduled task.
  ```bash
  schtasks /query /tn "GraphitDaemon"
  ```

---

## AST Indexing Issues

### No files indexed / empty graph

**Symptoms:** `graphit_ast_query` returns no results. `graphit_ast_schema` shows no node labels.

**Solutions:**
1. Run indexing explicitly:
   ```bash
   graphit ast index
   ```
2. Check if the `ast` module is disabled in config:
   ```bash
   graphit config get ast.disabled
   ```
3. Force a full reindex:
   ```bash
   graphit ast index --reset
   ```

### Unsupported language

**Cause:** The AST parser uses Tree-sitter grammars. Languages without a compiled grammar will be skipped during indexing.

**Currently supported languages include:** Go, Python, JavaScript, TypeScript, Java, Rust, C, C++, Ruby, PHP, C#, Swift, Kotlin, Scala, Lua, and others.

**Diagnosis:** Check the indexing output for skipped files or run with verbose logging.

### Large repository indexing is slow

**Solutions:**
1. Increase the worker count:
   ```bash
   graphit ast index --workers 8
   ```
2. Index a specific subdirectory instead of the full repo:
   ```bash
   graphit ast index --path src/
   ```
3. Disable source indexing to save space and time (source code will not be available via `graphit_ast_source`):
   ```bash
   graphit ast index --no-source
   ```
4. The AST database uses incremental indexing — only changed files are re-parsed on subsequent runs.

### Database locked / corruption

**Symptoms:**
```
database is locked
```

**Cause:** Multiple processes attempting to write to the SQLite database simultaneously, or an unclean shutdown.

**Solutions:**
1. Stop any running daemon or MCP server instances.
2. Reset the AST database:
   ```bash
   graphit ast index --reset
   ```
3. The database is stored in `.graphit/ast/`. Deleting the directory and reindexing is safe.

---

## AI & Embedding Issues

### ONNX Runtime initialization fails

**Symptoms:**
```
init ONNX Runtime: ...
```

**Cause:** The ONNX Runtime shared library could not be loaded. This is required for local embedding computation.

**Solutions:**
1. Ensure ONNX Runtime is installed on your system. Graphit bundles it in pre-compiled binaries, but building from source requires it separately.
2. Check that the shared library is in the expected path.
3. If local embeddings are not needed, the daemon will fall back to proxy-based embedding.

### Model download fails

**Symptoms:**
```
download model: ...
download tokenizer: ...
HTTP <status> from https://huggingface.co/...
incomplete download: wrote <N> of <M> bytes
```

**Cause:** The embedding model or tokenizer file could not be downloaded from Hugging Face.

**Solutions:**
1. Check your internet connection.
2. Verify you can reach `huggingface.co`:
   ```bash
   curl -I https://huggingface.co
   ```
3. If behind a corporate proxy, configure `HTTP_PROXY` / `HTTPS_PROXY` environment variables.
4. Check disk space — the model requires approximately 90 MB.
5. Clear the model cache and retry:
   ```bash
   rm -rf ~/.graphit/models/
   graphit sync
   ```

### Downloaded model is corrupt

**Symptoms:**
```
downloaded model too small — expected at least <N> bytes
downloaded tokenizer too small — expected at least <N> bytes
```

**Cause:** The download was interrupted or corrupted.

**Solutions:**
```bash
rm -rf ~/.graphit/models/
graphit sync
```

### Tokenizer load failure

**Symptoms:**
```
load tokenizer from <path>: ...
```

**Solutions:**
1. Delete and re-download the tokenizer:
   ```bash
   rm -rf ~/.graphit/models/
   ```
2. Ensure the tokenizer JSON file is valid and complete.

### Embedding proxy connection fails

**Symptoms:**
```
daemon embed request: ...
daemon embed: status <code>
empty embedding response from daemon
```

**Cause:** The MCP tool is trying to use the daemon's embedding server as a proxy, but the daemon is not running or the embedding server is not healthy.

**Solutions:**
1. Check daemon status:
   ```bash
   graphit daemon status
   ```
2. Check the embed port file:
   ```bash
   cat ~/.graphit/daemon/embed.port
   ```
3. Verify the embedding server is responding:
   ```bash
   curl http://127.0.0.1:$(cat ~/.graphit/daemon/embed.port)/health
   ```
4. Restart the daemon:
   ```bash
   graphit daemon stop
   graphit sync
   ```

### Invalid port in embed.port file

**Symptoms:**
```
invalid port in <path>: ...
```

**Cause:** The port file is corrupted or contains non-numeric content.

**Solutions:**
```bash
rm ~/.graphit/daemon/embed.port
graphit daemon stop
graphit sync
```

---

## Memory Issues

### Cannot determine user identity

**Symptoms:**
```
cannot determine user identity: ...
```

**Cause:** User-scoped memory requires a git user identity to generate a unique hash. Git is not configured with a user email.

**Solutions:**
```bash
git config --global user.email "your@email.com"
git config --global user.name "Your Name"
```

### Project not initialised (Memory)

**Symptoms:**
```
project not initialised
```

**Cause:** Memory operations require the project to have a ULID in `.graphit-lock.json`.

**Solutions:**
```bash
graphit init
```

### Invalid memory type

**Symptoms:**
```
invalid memory type "<type>"
```

**Cause:** The `type` parameter must be one of: `convention`, `correction`, `decision`, `tension`, `fact`, or `skill`.

**Solutions:** Check your tool call and use one of the valid types listed above.

### Memory git store errors

**Cause:** Memory persistence uses a git repository for sync. Issues arise when git authentication fails or the repository is unreachable.

**Solutions:**
1. Check hub configuration:
   ```bash
   graphit config get hub.repo
   graphit config get hub.repo --global
   ```
2. Test git credentials:
   ```bash
   git ls-remote <hub-repo-url>
   ```
3. If using SSH, verify your SSH key is configured:
   ```bash
   ssh -T git@github.com
   ```

---

## Knowledge Issues

### Knowledge wiki not found

**Symptoms:**
```
knowledge wiki not found
```

**Cause:** The knowledge wiki has not been indexed yet, or the specified context does not exist.

**Solutions:**
1. Run knowledge indexing:
   ```bash
   graphit knowledge index
   ```
2. Verify the docs directory exists:
   ```bash
   ls docs/
   ```
3. If using a custom docs directory, check the configuration:
   ```bash
   graphit config get docs.dir
   ```

### Knowledge export fails

**Symptoms:**
```
hub not configured: ...
project not initialised
```

**Cause:** Knowledge export pushes to the hub git repository, which must be configured.

**Solutions:**
1. Initialize the project if needed:
   ```bash
   graphit init
   ```
2. Configure the hub repository:
   ```bash
   graphit config set hub.repo <git-url>
   ```

---

## Hub Issues

### Registry unavailable

**Symptoms:**
```
registry unavailable: ...
```

**Cause:** The hub registry (remote git repository) cannot be accessed.

**Solutions:**
1. Check the hub repository URL:
   ```bash
   graphit config get hub.repo --global
   ```
2. Test connectivity:
   ```bash
   git ls-remote <hub-repo-url>
   ```
3. If the hub repo is private, configure authentication:
   - **SSH:** Ensure your SSH key is added to the agent:
     ```bash
     ssh-add ~/.ssh/id_ed25519
     ```
   - **HTTPS:** Use a credential helper:
     ```bash
     git config --global credential.helper store
     ```

### Artifact not found

**Symptoms:**
```
artifact "<id>" not found
```

**Solutions:**
1. List available artifacts:
   ```bash
   graphit hub list
   graphit hub search "<keyword>"
   ```
2. Check the artifact ID for typos.
3. Sync the registry to fetch the latest entries:
   ```bash
   graphit hub update
   ```

### Cannot load lockfile

**Symptoms:**
```
cannot load lockfile: ...
```

**Cause:** The `.graphit-lock.json` file is missing or corrupted.

**Solutions:**
1. Re-initialize the project:
   ```bash
   graphit init
   ```
2. If the file exists but is corrupt, check JSON validity:
   ```bash
   python3 -m json.tool .graphit-lock.json
   ```

---

## MCP Connection Issues

### MCP stdio server fails to start

**Symptoms:**
```
MCP stdio error: ...
```

**Cause:** The MCP server reads from stdin and writes to stdout using JSON-RPC. Issues arise when stdout is redirected or unavailable.

**Solutions:**
1. Verify the Graphit binary is accessible from your IDE's configured MCP server command.
2. Check your IDE's MCP server configuration:
   - **Claude:** Check `.claude/mcp.json`
   - **Cursor:** Check `.cursor/mcp.json`
   - **Gemini:** Check `.gemini/settings.json`
3. Ensure the binary path is absolute or resolvable from your `PATH`.

### Tool calls returning panics

**Symptoms:**
```
internal error (panic): ...
```

**Cause:** A tool handler panicked. All tool handlers are wrapped with panic recovery, so the error is returned gracefully instead of crashing the server.

**Solutions:**
1. Note the error details and the tool that caused it.
2. Check if the issue is related to a corrupted database:
   ```bash
   graphit ast index --reset
   ```
3. Try restarting the MCP server by restarting your IDE session.
4. Report the issue with the full error message.

### Daemon autostart from MCP

Every MCP tool call automatically tries to ensure the daemon is running. If it fails:
```
[MCP] Failed to ensure daemon is running: ...
```

This is logged to stderr and does not block the tool call. The tool will continue executing, but features that depend on the daemon (like embedding proxy) may not work.

---

## Dream Module Issues

### Dream status shows "inactive"

**Cause:** The dream module requires both:
1. The `dream.enabled` config set to `true`
2. The background daemon to be running

**Solutions:**
```bash
graphit config set dream.enabled true
graphit sync  # Ensures daemon is running
```

### Dream reports not generated

**Cause:** The dream module only activates after an idle timeout (configurable, default varies). The project must have pending subjects or discoverable work.

**Solutions:**
1. Check for pending subjects:
   ```bash
   graphit dream subject list
   ```
2. Add a dream subject manually:
   ```bash
   graphit dream subject add --title "Review recent changes" --body "Detailed instructions..."
   ```
3. Check dream status for timing:
   ```bash
   graphit dream status
   ```

---

## Getting Help

If your issue is not covered here:

1. **Check the daemon log** for detailed error messages:
   ```bash
   cat ~/.graphit/daemon/daemon.log
   ```

2. **Run a full sync** to trigger all modules and surface issues:
   ```bash
   graphit sync
   ```

3. **Use the MCP diagnostic tools:**
   - `graphit_daemon_status` — daemon health check
   - `graphit_ast_schema` — verify AST database is populated
   - `graphit_knowledge_list` — verify knowledge wiki exists
   - `graphit_memory_list` — verify memory store is accessible
   - `graphit_config_list` — review all configuration values

4. **Report issues** on the project's issue tracker with:
   - Graphit version (`graphit version`)
   - Operating system and architecture
   - Full error message or stack trace
   - Steps to reproduce
