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
  - "docs/guides/agent_hook_activation.md"
  - "docs/guides/daemon_operations.md"
  - "docs/guides/ai_models.md"
---

# Troubleshooting

This guide covers common issues you may encounter when using Graphit Code and how to resolve them. Issues are organized by module — jump to the relevant section for your problem.

> **Everywhere this guide writes `~/.graphit`, read the value of `GRAPHIT_GLOBAL_DIR`
> instead if you have set it.** That variable overrides the location of the global
> directory, and everything below — the config file, the daemon PID and log, the model
> cache, the extracted runtime — moves with it. Check it first when a path in these
> instructions does not exist:
>
> ```bash
> echo "${GRAPHIT_GLOBAL_DIR:-$HOME/.graphit}"
> ```
>
> See [Storage Layout](../architecture/storage_layout.md#moving-it-graphit_global_dir).

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
- [Agent Hook and Bootstrap Issues](#agent-hook-and-bootstrap-issues)
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
ls -la graphit.lock.json

# Check global configuration
graphit config --list --global

# Check project configuration
graphit config --list

# Inspect active provider/profile and UI network settings
graphit provider show <provider>
graphit account show
graphit config --get ui.host

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
2. If installed via the one-liner script, ensure `~/.local/bin` is in your `PATH`:
   ```bash
   export PATH="$PATH:$HOME/.local/bin"
   # Add permanently to ~/.bashrc or ~/.zshrc
   ```
3. If installed manually, move to a directory on your `PATH`:
   ```bash
   mkdir -p ~/.local/bin && mv graphit ~/.local/bin/
   # Or system-wide (requires sudo):
   # sudo mv graphit /usr/local/bin/
   ```
4. If built from source, check the install target for your OS:
   - **Linux:** `make install` → `/usr/local/bin/graphit` (or `make install PREFIX=$HOME/.local/bin`)
   - **macOS:** `make install-darwin` → `/usr/local/bin/graphit` (or `make install-darwin PREFIX=$HOME/.local/bin`)
   - **Windows (MSYS2):** `make install-windows` → `C:\Program Files\graphit\graphit.exe` (or `make install-windows PREFIX_WIN='C:\Tools\graphit'`)
5. On macOS, you may need to approve the binary in System Preferences → Security & Privacy after the first run.

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
- CGO is required for the graph engine, and the `lancedb` build tag for the search engine. A binary built without the tag links stubs whose error names the tag and the fix.

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
This ensures `graphit.lock.json` contains the immutable project ULID and completes initialization.
If an earlier stateful operation already created the minimal lockfile, `init` reuses its ULID. If
you are using an Agent adapter, specify it:
```bash
graphit init --agent claude
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
graphit config --list

# Check global config
graphit config --list --global

# Verify a specific key
graphit config --get <key>
graphit config --get <key> --global
```

### Agent adapter not detected

**Symptoms:** Rules are not installed for your Agent, or the wrong Agent format is used.

**Solutions:**
1. Set the Agent explicitly:
   ```bash
   graphit config agent claude          # Project-level
   graphit config agent cursor --global # Global default
   ```
2. Re-run sync to apply:
   ```bash
   graphit sync --agent claude
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

Install or inspect Graphit's user-scoped watchdog:

```bash
graphit daemon scheduler install
graphit daemon scheduler status
```

Linux uses the user crontab, macOS uses a LaunchAgent, and Windows uses Task Scheduler.
See [Daemon Operations and Monitoring](daemon_operations.md#optional-os-watchdog) for exact files,
intervals, and removal.

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
   graphit config --get ast.disabled
   ```
3. Force a full reindex:
   ```bash
   graphit ast index --reset
   ```

AST indexing and embedding cycles share a cross-process lifecycle lock. This covers
full indexing, incremental updates from the filesystem watcher, and `--reset`. A
reset waits for an in-flight embedding cycle, rebuilds the store exclusively, and
the daemon reopens the published store on its next embedding cycle. A manual daemon
restart is not required.

### Unsupported language

**Cause:** The AST parser uses Tree-sitter and ANTLR v4 grammars. Languages without an installed grammar will be skipped during indexing.

**Currently supported languages (44):**
- **Tree-sitter (39):** Bash, C, C++, C#, Clojure, CSS, Dart, Dockerfile, Elixir, Go, GraphQL, Groovy, Haskell, HCL, HTML, Java, JavaScript, JSON, Julia, Kotlin, Lua, Objective-C, PHP, Protocol Buffers, Python, R, Ruby, Rust, Scala, SQL, Svelte, Swift, TOML, TSX, TypeScript, Vue, XML, YAML, Zig
- **ANTLR (5):** PL/SQL, PostgreSQL, T-SQL, DB2, COBOL 85

Markdown is not on the list: no shipped query file claims `.md`, `.markdown` or
`.mdx`, so documents are never indexed into the code graph. They are the knowledge
wiki's — search them with `graphit knowledge search`, not with a Cypher query. The
`tree-sitter-markdown` grammar is still compiled in, so a project that wants
markdown structure in the graph adds its own `markdown.yaml` under
`ast.queries_dir`.

**Diagnosis:** Check the indexing output for skipped files or run with verbose logging.

### An Oracle / T-SQL / DB2 repository indexes nothing

**Cause:** the SQL dialect grammars — `plsql`, `postgresql`, `db2`, `tsql`, `plpgsql` —
are **exclusive**: they claim no file extensions, so nothing reaches them by extension
and nothing falls back to them. A `.sql` file is parsed by the tree-sitter `sql`
grammar, which reads standard DDL and DML and produces nothing for a PL/SQL package
body; `.pks`, `.pkb`, `.prc`, `.db2` and `.tsql` are not discovered at all.

This is deliberate. Four ANTLR grammars used to claim `.sql` and were tried in turn
whenever tree-sitter came back empty, which made the dialect a *guess* — decided by
whichever grammar happened to extract an entity first — and cost up to four full
parses per file.

**Solution:** name the dialect.

```bash
# Oracle: the .sql files and the package/procedure files
graphit config ast.grammar ".sql=antlr-plsql,.pks=antlr-plsql,.pkb=antlr-plsql,.prc=antlr-plsql,.fnc=antlr-plsql,.trg=antlr-plsql"

# SQL Server
graphit config ast.grammar ".sql=antlr-tsql"

# PostgreSQL
graphit config ast.grammar ".sql=antlr-postgresql"

graphit ast index --reset
```

Use the configuration key rather than the `--grammar` flag when the extension is one
only a dialect claims: file discovery reads the key, and the flag is applied at parse
time, after discovery already decided what to offer. Check what is in effect with
`graphit config --get ast.grammar`.

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

### `failed to open database with status 1`

**Symptoms:**
```
failed to open database with status 1
```

**Cause:** The graph has no database file to lock — it is a Parquet bundle
(`graph.icebug/`) mounted fresh, in memory, on every query — but the engine's C API
reports every open failure as this one opaque code, with no message channel to say
which. In practice this means a query landed in the brief window while the daemon is
publishing a freshly rebuilt bundle (an atomic directory rename). It is transient: the
graph backend already retries up to 5 times over roughly 750 ms before surfacing this
error, so seeing it means that budget was exceeded, not that nothing is there.

**Solutions:**
1. Retry the query — a rebuild that was still publishing when the budget ran out will
   have finished a moment later.
2. If it persists, check whether the daemon is stuck mid-rebuild:
   ```bash
   graphit daemon status
   ```
3. Reset the AST store:
   ```bash
   graphit ast index --reset
   ```
4. The store lives under the global graphit directory (`~/.graphit` by default, or
   `GRAPHIT_GLOBAL_DIR` if set), not inside the project — `graphit ast index --reset`
   finds and rebuilds it without needing the exact path.

---

## AI & Embedding Issues

### ONNX Runtime initialization fails

**Symptoms:**
```
init ONNX Runtime: ...
```

**Cause:** The ONNX Runtime core library could not be loaded. Local embedding and local rerank both
use it.

**Solutions:**
1. A release launcher extracts the native files beside the versioned core. Confirm the platform
   payload contains `libonnxruntime.so.1.29.0` on Linux, `onnxruntime.dll` on Windows, or
   `libonnxruntime.1.29.0.dylib` on macOS.
2. A source build must run `make native-deps` or expose the matching library directory through
   `LD_LIBRARY_PATH`, `PATH`, or `DYLD_LIBRARY_PATH`.
3. For explicit `cuda`, Linux also needs `libonnxruntime_providers_shared.so` and
   `libonnxruntime_providers_cuda.so`; Windows needs the equivalent provider DLLs. The host must
   supply compatible NVIDIA driver/CUDA runtime dependencies. Explicit CUDA does not fall back.
4. To force a diagnostic CPU run, set the affected service only:
   ```bash
   graphit --non-interactive provider update NAME \
     --embedding-device cpu --rerank-device cpu
   ```

### Provider configuration rejects a local ONNX device selection

**Symptoms:** `graphit provider add` or `graphit provider update` reports an invalid embedding or
rerank ONNX execution setting, or says device flags require the corresponding mode to be `local`.

**Cause:** Provider configuration uses the same validation contract as model-session construction. Device IDs must
be non-negative. `coreml` is valid only on macOS and requires ID `0`; macOS `auto` also requires ID
`0`. A service routed to `direct`, `broker`, or `disabled` has no ONNX block and rejects local-device
flags.

**Solutions:**
1. For portable acceleration with CPU fallback, accept the interactive defaults or configure them:
   ```bash
   graphit --non-interactive provider add workstation --type local \
     --embedding-mode local \
     --embedding-device=auto --embedding-device-id=0 \
     --rerank-mode local \
     --rerank-device=auto --rerank-device-id=0
   ```
2. Use `cuda` only on a machine with a working CUDA provider and choose a zero-based NVIDIA device
   index. Use `coreml` only on macOS. Explicit accelerators fail instead of falling back.
3. Use `cpu` to disable acceleration for one service without changing the other.
4. Rerun interactive `provider update NAME` to see the provider's current values in brackets;
   pressing Enter keeps them. A new local service starts at `auto` and `0`.
5. With no active profile, inspect `graphit provider show local`. Setup or the next AI resolution
   creates this real provider with `auto`/`0` if it is absent; use `provider update local` to force
   CPU or select an accelerator.

### Model download fails

**Symptoms:**
```
Embedding model not ready: download model: ...
download tokenizer: ...
HTTP <status> from https://huggingface.co/...
incomplete download: wrote <N> of <M> bytes
```

**Cause:** A required artifact in the selected embedding or rerank manifest could not be
downloaded from one of its sources. Model bundles are **not** shipped inside the binary. Built-in
defaults use Hugging Face; operator-defined manifests may use other HTTPS URLs.

The command that triggered acquisition fails with a non-zero exit status. This can be setup or
daemon startup for a manifest with `fetch_policy: setup`, or the first model operation for
`fetch_policy: on_demand`. Rerun that same command after fixing the network or cache.

**Solutions:**
1. Check your internet connection.
2. Verify you can reach `huggingface.co`:
   ```bash
   curl -I https://huggingface.co
   ```
3. If behind a corporate proxy, configure `HTTP_PROXY` / `HTTPS_PROXY` environment variables.
4. Check disk space for every artifact plus its temporary download.
5. Re-run the operation that owns the manifest's fetch policy:
   ```bash
   graphit setup          # fetch_policy: setup
   graphit ast embed      # fetch_policy: on_demand for embedding
   ```
6. Or move the damaged cache aside and let it start over:
   ```bash
   mv ~/.graphit/models ~/.graphit/models.damaged
   graphit ast embed
   ```
7. With no route to Hugging Face at all, supply the files by hand. See
   [Air-gapped machines](#air-gapped-machines) below.

### The model downloads again after an upgrade

It should not. The catalog lives at `~/.graphit/models/<id>/`, which is
outside the per-version runtime directory that the launcher wipes on upgrade
(`~/.graphit/runtime/<version>/`). If a download starts on every upgrade, check
that nothing is cleaning `~/.graphit/models/`.

### Air-gapped machines

A machine with no model-source route needs a complete bundle before its first local operation.
Create `~/.graphit/models/<id>/manifest.json` with `fetch_policy: "never"`, copy every required
artifact to the manifest path, and select the bundle:

```bash
graphit config --global models.embedding.id offline-embedder
graphit --non-interactive provider add offline --type local \
  --embedding-device cpu --rerank-device cpu
graphit --non-interactive login \
  --profile offline --provider offline --username local
graphit ast embed
```

Required files must match their SHA-256 and minimum-size declarations. Missing or invalid artifacts
fail clearly, and `never` performs no network request. Full detail is in
[Air-Gapped Deployments](private_brand_customization.md#air-gapped-deployments) and
[Local model manifests](ai_models.md#local-model-manifests).

### Downloaded model is corrupt

**Symptoms:**
```
artifact <role> is smaller than required
artifact <role> sha256 mismatch
```

**Cause:** The download was interrupted or corrupted.

**Solutions:**
Move the affected bundle aside, then retry the operation. For custom manifests, correct the source
URL/hash or restore the operator-provided files before retrying.

### Tokenizer load failure

**Symptoms:**
```
load tokenizer from <path>: ...
```

**Solutions:**
1. Verify that `tokenizer.artifact` names an artifact in the selected manifest.
2. Ensure its declared format is supported and the tokenizer JSON is valid and complete.
3. Move the affected bundle aside to re-download it only when its fetch policy and sources permit.

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
2. Check that the Unix socket exists:
   ```bash
   ls -l ~/.graphit/daemon/embed.sock
   ```
3. Restart the daemon:
   ```bash
   graphit daemon stop
   graphit sync
   ```

The embedding proxy is newline-delimited JSON over a Unix socket. It has no HTTP port file or
health URL. `mcp.port` belongs to the separate authenticated MCP HTTP listener.

### MCP client gets HTTP 401

**Cause:** The client is not sending an accepted bearer. The daemon runtime key changes at each
start; profile credentials can also be stale or belong to a non-active account.

**Solutions:**

1. Open **System → Daemon** and copy the full active value from **MCP bearer key**, or read
   `~/.graphit/daemon/mcp.key` on the host.
2. Send it as `Authorization: Bearer <key>` to the `/mcp` endpoint.
3. For a stable credential, log in the active profile with `--mcp-key`; for OIDC, verify refresh
   succeeds and send the renewed access token. `graphit account show` redacts both.

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

**Cause:** Memory operations require the project to have a ULID in `graphit.lock.json`.

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

### Memory is unexpectedly local

**Cause:** Memory uses an authoritative LanceDB table directly on S3 only when the active provider
has a resolved S3 topology and credentials. Without S3 it falls back to the global filesystem;
anonymous user memory is always local.

**Solutions:** Configure direct S3 on a local provider, web-identity STS on an OIDC provider, or use
a Broker provider whose discovery advertises `graphit-s3-credentials-v2`. Re-login if the provider
revision changed, then inspect the redacted account state. Broker credential expiry is managed only
in process memory and is intentionally absent from account output.

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
   graphit config --get knowledge.docs_dir
   ```

### A document exists but is not in the wiki

**Symptoms:** `knowledge search` and `wiki browse` never return a page you know you wrote, and reindexing changes nothing.

**Cause:** It is outside the wiki's scope, which is `knowledge.docs_dir` (default `docs/`) plus the project root's README — and nothing else. This is not a stale index: the file was never read. Earlier versions defaulted this key to `.`, the whole project, so a document anywhere used to be picked up.

**Solutions:**
1. Check where the wiki is looking:
   ```bash
   graphit config --get knowledge.docs_dir
   ```
2. Move the document under that directory — the usual answer, since that is where documentation belongs.
3. Or point the key at where your documentation actually lives:
   ```bash
   graphit config knowledge.docs_dir documentation
   ```
4. Or restore the old whole-project behaviour:
   ```bash
   graphit config knowledge.docs_dir .
   ```
5. Missing the **root README** specifically? It is indexed by default; check that it has not been switched off:
   ```bash
   graphit config --get knowledge.include_readme
   ```
   Only the root README is in scope. A `README.md` in a subdirectory is an ordinary file.
6. Still nothing? Then it *is* a scope-independent problem — check `.wikiignore`, and that the extension is in `knowledge.extensions`.

### Documentation files are missing from the AST code graph

**Symptoms:** `ast query` returns no `File` node for a path under `docs/`, and `ast source` reports it is not indexed.

**Cause:** Intentional. The documentation tree belongs to the knowledge wiki, so the AST pipeline excludes `knowledge.docs_dir` by default (`ast.index_docs=false`). Query the wiki for those documents instead — it chunks and cross-links prose in ways the code graph cannot.

**Solutions:**
1. Search the wiki rather than the graph — `knowledge search`, `wiki browse`, `wiki source`.
2. If you genuinely need structural queries over files in the docs tree — `.proto` or `.graphql` schemas kept there, for instance:
   ```bash
   graphit config ast.index_docs true
   graphit ast index --reindex
   ```
3. `!docs/` in `.astignore` will **not** work: built-in default patterns are applied last, and gitignore semantics are last-match-wins, so they outrank the project's own patterns. Use the config key.
4. One-off, without changing configuration — an explicit path is taken literally:
   ```bash
   graphit ast index --path docs
   ```

### A whole language is missing from the AST code graph

**Symptoms:** `ast query` returns no node at all for a language you know is in the repository — `MATCH (f:File) RETURN f.lang, count(f)` simply does not list it — and no ignore pattern explains it. Or `ParseWithGrammar` fails with `grammar disabled by configuration: <name>`.

**Cause:** most likely a grammar switched off by configuration. `ast.grammars_blacklist` disables the grammars it names; `ast.grammars_whitelist`, when non-empty, disables everything it does *not* name. Both are inherited from the global config, so the answer may not be in this project's lockfile at all.

**Solutions:**
1. Read both keys, at both levels — the project's, then the machine's:
   ```bash
   graphit config --get ast.grammars_blacklist
   graphit config --get ast.grammars_whitelist
   graphit config --list --global
   ```
   Check the environment too: `GRAPHIT_AST_GRAMMARS_BLACKLIST` and `GRAPHIT_AST_GRAMMARS_WHITELIST` outrank both files.
2. Clear the one that is wrong, then reindex so the files are discovered again:
   ```bash
   graphit config --unset ast.grammars_blacklist
   graphit ast index
   ```
3. **A typo in either key is silent.** An entry matching no known grammar is deliberately inert — the lists are read in processes that may not have a grammar pack installed yet — so `ast.grammars_blacklst` (or `pyton`) disables nothing and reports nothing. Verify the key name letter by letter against `graphit config --list`; a value you set that does not appear there went to a different key.
4. The name has to match the language, the grammar, or the grammar without its `tree-sitter-` / `antlr-` prefix. `yaml` and `tree-sitter-yaml` are the same language; `yml` is neither.
5. Neither key set and the language still absent? Then it is one of the other two axes — no query file claims the extension, or a path pattern excludes it. See [ignore_files](ignore_files.md#excluding-a-language-rather-than-a-path).
6. Nodes from before the key was set survive a **scoped** index (`--path`), because the tree is never walked and nothing can be pruned. A full `graphit ast index` removes them.

### Knowledge export fails

**Symptoms:**
```
hub not configured: ...
project not initialised
```

**Cause:** Knowledge export publishes through the active provider's S3 store. A Broker with S3
disabled validly omits `graphit-s3-credentials-v2`, so login works and ordinary storage is local,
but remote export is unavailable. For export, the Broker must advertise the capability, authorize
the principal, and issue a valid restricted STS session; direct providers must resolve their
configured credentials.

**Solutions:**
1. Initialize the project if needed:
   ```bash
   graphit init
   ```
2. Configure a named broker provider and login:
   ```bash
   graphit provider add company --type oidc --issuer https://id.example/realms/acme \
     --client-id graphit-cli --broker-endpoint https://broker.example.com \
     --embedding-mode broker --rerank-mode broker
   graphit login --provider company --profile alice
   ```

---

## Hub Issues

### Registry unavailable

**Symptoms:**
```
registry unavailable: ...
```

**Cause:** The broker-backed Hub control plane cannot be accessed or authorization cannot be
revalidated. Graphit does not fall back to a cached positive decision.

**Solutions:**
1. Check the provider and active account:
   ```bash
   graphit provider show <provider>
   graphit account show
   ```
2. Verify broker endpoint DNS/TLS and discovery output.
3. Check the broker's private route key, object-store endpoint, bucket policy, and ACL logs;
   none of these settings belongs in Graphit provider/login configuration.
4. Re-run `graphit login` if the OIDC session or static broker key is invalid.
5. For multi-user OIDC deployments, verify that the MCP bearer is valid for the configured MCP
   audience and that relay or RFC 8693 exchange produces a bearer accepted independently by the
   broker. Graphit never trusts user/team values supplied in a request body.

### Hub lists no projects

An empty result is valid deny-by-default behavior. First inspect broker discovery. If it advertises
`graphit-hub-access-v1`, check normalized grants in the broker administration UI/API and its OIDC
claim mapping; the broker SQL database is authoritative and Graphit intentionally does not inspect
S3 ACL documents. Otherwise, check whether any applicable document exists at
`v2/global/projects.json`, `v2/authenticated/projects.json` for a non-anonymous subject,
`v2/users/<user-id>/projects.json`, or the subject's `v2/teams/<team-id>/projects.json` keys. A
missing or empty file contributes no grant from that level, and Graphit never creates an implicit
global `*` grant. Anonymous reads `v2/anonymous/projects.json` instead of authenticated, user, or
team documents.

Add the intended SQL resource grant in Broker mode, or edit the applicable standalone grant
document outside Broker mode. Never maintain both as mirrors.

### Hub access is forbidden for a known ID

Knowing a project ULID, artifact ID, version, or logical object key does not grant access. Confirm that the
current trusted subject has an exact ULID, matching name-prefix, or all-project grant. A lockfile
installation claim can outlive a grant; reopening its AST or Knowledge mount correctly fails after
revocation.

### Hub cache is stale

`~/.<brand>/hub/cache/` is disposable discovery metadata. Remove the affected Hub/subject cache or
let its TTL expire, then retry while the control plane is reachable. Cached rows may be shown as
stale offline, but they cannot authorize details, content, installation, or mounts.

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
3. Refresh authorized discovery metadata:
   ```bash
   graphit hub update
   ```
4. If the artifact ID is ambiguous across projects, qualify it with the publishing project ULID and
   type.

### Project name already reserved

The friendly name is globally unique within one Hub. Choose another name or complete an interrupted
rename repair. Do not change the local ULID: it is the project's stable identity and remains valid
even when name registration fails.

### Cannot load lockfile

**Symptoms:**
```
cannot load lockfile: ...
```

**Cause:** The `graphit.lock.json` file is missing or corrupted.

**Solutions:**
1. Re-initialize the project:
   ```bash
   graphit init
   ```
2. If the file exists but is corrupt, check JSON validity:
   ```bash
   python3 -m json.tool graphit.lock.json
   ```

---

## UI Network Issues

### Remote browser cannot connect

Check `graphit config --get ui.host`. `127.0.0.1` accepts only local connections;
`0.0.0.0` accepts reachable IPv4 interfaces, subject to firewall rules. The port is
selected automatically, so use the URL printed by `graphit ui`.

### Browser reports a CORS error

The page's exact origin, including scheme and non-default port, must appear in the
comma-separated `ui.allowed_origins`. A configured list replaces the localhost default.
The embedded UI itself uses same-origin `/api` and normally needs no entry. `*` allows
every browser origin and is unsafe for most deployments.

The UI has no authentication. A curl request can succeed even when browser CORS blocks a
page, because CORS is browser enforcement rather than server authorization. Protect remote
access with a firewall, VPN, or authenticated TLS reverse proxy. See
[S3 Credentials and UI Network Configuration](s3-and-ui-network.md).

---

## MCP Connection Issues

### MCP stdio server fails to start

**Symptoms:**
```
MCP stdio error: ...
```

**Cause:** The MCP server reads from stdin and writes to stdout using JSON-RPC. Issues arise when stdout is redirected or unavailable.

**Solutions:**
1. Verify the Graphit binary is accessible from your Agent's configured MCP server command.
2. Check your Agent's MCP server configuration:
   - **Claude:** Check `.mcp.json`
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
3. Try restarting the MCP server by restarting your Agent session.
4. Report the issue with the full error message.

### Daemon autostart from MCP

The stdio MCP process ensures the daemon when it starts and retries before tool handling. If it fails:
```
[MCP] Failed to ensure daemon is running: ...
```

This is logged to stderr and does not block the tool call. The tool continues, and embedding can
fall back to direct provider construction, but background indexing and recurring maintenance remain
unavailable until the daemon starts.

---

## Agent Hook and Bootstrap Issues

### Graphit tools exist, but the agent ignores Graphit-first instructions

**Cause:** MCP availability and hook execution are independent. The MCP configuration can expose `graphit_*` tools while the host withholds, disables, or has not yet reloaded the project hook that injects Graphit's mandates and installed Hub rules.

**Solutions:**

1. Run `graphit sync` from the repository.
2. Complete the trust, enablement, reload, and verification steps for your adapter in [Activate Graphit Hooks in Each Agent](agent_hook_activation.md).
3. Start a new agent session; an already-running session does not retroactively receive its start hook.
4. Check both the hook and the Graphit MCP tools. Seeing only one does not prove the other is active.

### Main agent uses Graphit, but a subagent does not

**Cause:** The host can give a child different MCP servers, tool allowlists, or permissions. Instructions injected at a subagent boundary cannot expose a tool the host omitted.

**Solutions:**

1. Inspect the custom agent's tool allowlist or deny list and permit the Graphit MCP tools when Graphit-first behavior is required.
2. Follow the adapter-specific subagent note in [Agent Hook Activation](agent_hook_activation.md).
3. For Cursor Cloud, configure MCP at the team/enterprise layer or use a local/self-hosted agent; cloud agents do not run `sessionStart` and cannot use your local MCP configuration.

If the host does not provide the hook boundary or Graphit tools to that child, native-tool fallback is expected. Do not block the subagent solely because Graphit is unavailable.

---

## Dream Module Issues

### Dream status shows "inactive"

**Cause:** The dream module requires both:
1. `modules.dream` set to `true` — dream is **opt-in**, so an absent value means off
2. The background daemon to be running

**Solutions:**
```bash
graphit config modules.dream true
graphit sync  # Ensures daemon is running
```

### Dream reports not generated

**Cause:** The Dream module only activates after an idle timeout (configurable, default varies). Reports contain skill-generation findings, conversation analysis results, and newly created memories or skills. Graphit Task is not a Dream input.

**Solutions:**
1. Check dream status for timing:
   ```bash
   graphit dream status
   ```

### Agents do not see the same tasks

**Cause:** Agents are resolving different project identities, S3 settings, or `task.prefix` values.

**Solutions:**
```bash
graphit config --get task.prefix
graphit provider show <provider>
graphit account show
graphit task list
```
Also compare `graphit.lock.json` project IDs, active profile/provider, and `GRAPHIT_TASK_PREFIX`.
Task has no
repository Markdown replica to reconcile.

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
