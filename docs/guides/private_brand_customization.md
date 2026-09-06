# Private Branding and Deployment

Graphit Code can be compiled as a private white-label distribution and connected
to an S3-compatible service inside your network. This guide separates build-time
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
| `DEFAULT_HUB_BUCKET` | `brand.DefaultHubBucket` | Compiled default S3 bucket |
| `DEFAULT_HUB_REGION` | `brand.DefaultHubRegion` | Compiled default region |
| `DEFAULT_HUB_ENDPOINT` | `brand.DefaultHubEndpoint` | Compiled default S3-compatible endpoint |
| `SELF_UPDATE_URL` | `brand.SelfUpdateURL` | Private update source when provided |

Example:

```bash
make build-local \
  BRAND=acme-code \
  DISPLAY_NAME="Acme Code Intelligence" \
  DEFAULT_HUB_BUCKET=acme-graphit-prod \
  DEFAULT_HUB_REGION=us-east-1 \
  DEFAULT_HUB_ENDPOINT=https://s3.internal.example
```

The compiled S3 values are defaults, not credentials. Operators can override them
through environment, project config, or global config. Do not embed access keys or
secrets in linker flags or release workflows.

The public `.github/workflows/release.yml` deliberately omits all three
`DEFAULT_HUB_*` variables. Their empty Makefile defaults leave the released binary
in local-only/operator-configured mode until setup, environment, or layered config
provides an S3 location. A private release may pass `DEFAULT_HUB_BUCKET`,
`DEFAULT_HUB_REGION`, and `DEFAULT_HUB_ENDPOINT`; `DEFAULT_HUB_REPO` is obsolete and
is not part of the build contract.

Release builds run natively on each platform because the LanceDB search library
cannot be cross-compiled. The Windows job must call `make build-windows-native` on
its MSYS2 runner; that target bundles the Windows-native LanceDB, LadybugDB, and
ONNX Runtime libraries. Do not replace it with a Windows cross-build that omits the
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

Use a private AWS S3 bucket or an S3-compatible endpoint such as an internal MinIO
deployment. Hub v2 carries a global name directory, deny-by-default global/user/team grant
documents, and project data rooted below immutable ULIDs. Versioned artifacts, events, published
knowledge, mounted graph/search stores, project memory, and project Tasks remain inside their
project prefix; user memory remains inside its user prefix.

For a single-user or workload deployment, configure the branded equivalents of
`GRAPHIT_HUB_SUBJECT_USER` and `GRAPHIT_HUB_SUBJECT_TEAMS` globally. A multi-user service should
bind the authenticated subject to each request instead; it must not accept these values from an API
payload or project file.

```bash
acme-code setup
```

Setup collects the bucket, region, endpoint, and optional access/secret pair. A
complete pair is written to the global config. Leaving either credential blank
removes both explicit values and uses the AWS provider chain, which is preferred
for workload roles and short-lived credentials.

Explicit secrets are stored as plain text in the owner-only global config file.
They are redacted from config output but are not encrypted at rest. These credentials authenticate
the workload, not a user. A multi-user ecosystem needs trusted user/team identity plus a Hub
service or temporary credentials scoped to authorized project ULIDs. Bucket policy, endpoint TLS,
identity/role policy, and network segmentation remain the real data security boundary. See
[S3 Credentials and UI Network Configuration](s3-and-ui-network.md) and
[Hub S3 Object Layout](../specs/hub-s3-object-layout.md), and
[Hub Access Control](../specs/hub_access_control.md).

### Prefix isolation

Set `hub.prefix` when multiple teams or environments share a bucket:

```bash
acme-code config --global hub.prefix engineering/prod
```

Use separate prefixes or buckets for production, staging, and unrelated trust
domains. A prefix is an object namespace, not an authorization boundary unless the
bucket policy enforces it.

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

The local embedding engine does not require an LLM API key. S3 credentials, when
used, are infrastructure credentials and are distinct from model-provider API
keys. A private distribution can keep embeddings local while still choosing
whether prompt-completion integrations are disabled, proxied, or explicitly
configured.

## Air-gapped deployments

The launcher embeds runtime binaries and query YAMLs, but the embedding model is
downloaded during `setup`. For an offline image, pre-stage both model files in the
branded global directory:

```text
~/.<brand>/models/coderankembed/model.onnx
~/.<brand>/models/coderankembed/tokenizer.json
```

`setup` detects the complete cache and skips the network download. Also provide:

- the application/launcher artifacts for every target platform;
- any required native libraries and grammar packages;
- an internal S3-compatible endpoint, or no bucket for local-only operation;
- an internal update source if self-update is enabled; and
- firewall/DNS rules that prevent unintended egress.

Validate the branded binary in a clean environment before release: run setup,
initialize a sample project, perform an S3 publish/install cycle when remote mode
is enabled, and verify UI access through the intended network boundary.
