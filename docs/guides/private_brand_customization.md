# Private Branding and Deployment

Graphit Code can be compiled as a private white-label distribution and connected
to a Graphit Broker inside your network. This guide separates build-time
branding from runtime operator configuration and calls out the network boundaries
that still need protection.

## Build-time branding

The Makefile injects brand fields through Go linker flags. The relevant variables
are:

| Make variable | Runtime field | Purpose |
|---|---|---|
| `BRAND` | `brand.Brand` | Binary/config-directory prefix |
| `DISPLAY_NAME` | `brand.DisplayName` | Human-readable product name |
| `GITHUB_REPO` | `brand.GitHubRepo` | Source/update metadata when applicable |
| `SELF_UPDATE_URL` | `brand.SelfUpdateURL` | Private update source when provided |

Example:

```bash
make build-local \
  BRAND=acme-code \
  DISPLAY_NAME="Acme Code Intelligence"
```

There are deliberately no compiled Hub bucket, region, or endpoint variables. A private client
contains no storage topology: it learns only a broker endpoint from its named provider. Configure
object storage in the broker deployment, never in linker flags or client release workflows.

Release builds run natively on each platform because the LanceDB search library
cannot be cross-compiled. The Windows job must call `make build-windows-native` on
its MSYS2 runner; that target bundles the Windows-native LanceDB, LadybugDB, and
ONNX Runtime core/shared/CUDA provider libraries. Do not replace it with a Windows cross-build that omits the
search engine.

## Global directory and environment names

The brand controls the default global directory and environment prefix. A build
with `BRAND=acme-code` uses `~/.acme-code/` and `ACME_CODE_*` variables. The
environment-only `<PREFIX>_GLOBAL_DIR` can relocate that directory before config
resolution begins.

Global state includes configuration, global rules, local authoritative stores, compiled AST/wiki
stores, runtime payloads, models, daemon metadata, managed file artifacts, and the bounded Hub cache
at `~/.<brand>/hub/cache/`. The cache is isolated by Hub and trusted subject, is safe to remove, and
is neither an ACL authority nor a complete registry mirror. Project-local source,
`graphit.lock.json`, rule/query overrides, and generated runtime state remain
separate. See [Storage Layout](../architecture/storage_layout.md).

## Setting up private collaboration ecosystems

Deploy Graphit Broker in front of a private AWS S3 bucket or an S3-compatible service such as
MinIO. Configure bucket, region, endpoint, base prefix, permanent STS caller key, and assumable role
only on the Broker; manage normalized resource grants through its administration UI/API and SQL
database. Hub v2 uses logical keys rooted below immutable project ULIDs, while remote Memory and
Task use their own authoritative LanceDB prefixes.

For a single-user or workload deployment, configure the branded equivalents of
the active named account profile. A multi-user service should
bind the authenticated subject to each request instead; it must not accept these values from an API
payload or project file.

```bash
acme-code setup
```

Setup collects installation preferences, ensures the branded default provider `local`, and
provisions model bundles whose manifest requests the `setup` fetch policy. Create another named
local or OIDC provider with
`--broker-endpoint`, `--embedding-mode broker`, and `--rerank-mode broker`, then login with OIDC or
`--broker-key`; login activates that profile. Broker configuration rejects local, direct, disabled,
or omitted AI service modes; `search.rerank` still controls whether reranking runs. The
owner-only global auth file stores identity sessions and broker/MCP/direct-AI keys, never AWS
credentials or bucket configuration. Broker ACL, bucket/IAM policy, endpoint TLS, and network
segmentation form the remote data boundary. See
[Broker Storage and UI Network Configuration](s3-and-ui-network.md) and
[Hub S3 Object Layout](../specs/hub-s3-object-layout.md), and
[Hub Access Control](../specs/hub_access_control.md).

### Storage isolation

Choose prefixes, buckets, accounts, or broker instances on the broker side. The broker may route
organizations and projects dynamically without updating or re-logging client providers.

## UI network hardening

The unified UI binds to `127.0.0.1` by default and selects a free port. Browser CORS
remains limited to localhost until `ui.allowed_origins` is explicitly configured.
The server has no authentication, and CORS does not stop scripts or direct network
clients.

For a workstation-only private build:

```bash
acme-code config --global ui.host 127.0.0.1
```

For a shared deployment, keep the service on a private network and put it behind
an authenticated TLS reverse proxy:

```bash
acme-code config --global ui.host 0.0.0.0
acme-code config --global ui.allowed_origins https://code.acme.internal
```

Do not expose the raw server directly to the public Internet. Configure firewall
rules, VPN access, authentication, request limits, and TLS at the proxy or platform
boundary.

## Private model and API policy

The local embedding engine does not require an LLM API key. Cloud storage credentials remain in
the broker and are distinct from direct model-provider API keys. A private distribution can keep
embeddings local only when it configures no broker; a broker-configured provider must route both
embedding and rerank through that broker. Prompt-completion integrations remain independent and
may be disabled, proxied, or explicitly configured.

## Air-gapped deployments

The launcher embeds runtime binaries, platform ONNX providers, and query YAMLs, but model bundles
are separate. For an offline image, install a manifest and all of its required artifacts in the
branded global directory. For the built-in embedding default that is:

```text
~/.<brand>/models/coderankembed/model.onnx
~/.<brand>/models/coderankembed/tokenizer.json
```

Custom bundles live at `~/.<brand>/models/<id>/manifest.json`. Use `fetch_policy: "never"` for a
strictly offline bundle; a custom manifest may instead provide HTTPS source URLs plus SHA-256
digests when controlled downloading is allowed. Select it with `models.embedding.id` or
`models.rerank.id`. The manifest is hardware-neutral; select CPU/CUDA/CoreML through the `device`
and `device_id` flags for each local service when adding/updating a provider. With no active
profile, the persisted provider `local` uses `auto`/`0` and is recreated if absent. The first client validates the
complete bundle and skips the network when every artifact is present. Also provide:

- the application/launcher artifacts for every target platform;
- any required native libraries and grammar packages;
- an internal Broker with STS-capable S3 storage, direct provider S3 configuration, or no S3 for local-only operation;
- an internal update source if self-update is enabled; and
- firewall/DNS rules that prevent unintended egress.

Validate the branded binary in a clean environment before release: run setup,
initialize a sample project, perform an S3 publish/install cycle when remote mode
is enabled, and verify UI access through the intended network boundary.
