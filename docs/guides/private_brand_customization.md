---
title: Private Branding and Deployment
updated: 2026-10-30
references:
  - target:
      type: task
      id: tsk-cab0
    relation: corrects
  - target:
      type: session
      id: ses-c98a
    relation: relates_to
  - target:
      type: task
      id: tsk-8d62
    relation: supports
  - target:
      type: task
      id: tsk-6997
    relation: supports
---

# Private Branding and Deployment

Graphit Code can be compiled as a private white-label distribution and connected
to a Graphit Broker inside your network. This guide separates build-time
branding from runtime operator configuration and calls out the network boundaries
that still need protection.

The maintained product builds through Cargo. The compiled brand selects the
[namespace and path context](#rust-namespace-and-path-context); native packaging
and model provisioning are separate operations.

## Build-time branding

The App build script validates these public build inputs:

| Environment variable | Purpose |
|---|---|
| `GRAPHIT_BUILD_BRAND` | Validated namespace; defaults to `graphit` |
| `GRAPHIT_BUILD_VERSION` | Printable product version; release builds require an explicit value other than `dev` |

Example:

```bash
GRAPHIT_BUILD_BRAND=acme-code make build
./target/debug/graphit --help
```

Cargo and the maintained package retain the executable filenames `graphit` and
`graphit-model-server`. The compiled namespace does not rename these files or
restyle the React UI. Display names, repository URLs and private update endpoints
are not inputs to this build script. Use the actual package layout described in
[native runtime packaging](../development/native-runtime-packaging.md) and the
[self-update contract](../specs/rust_self_update.md).

There are deliberately no compiled Hub bucket, region, or endpoint variables. A private client
contains no storage topology: it learns only a broker endpoint from its named provider. Configure
object storage in the broker deployment, never in build metadata or client release workflows.

The release workflow builds Rust executables on each platform and packages their
native assets. Use `make release` with explicit product version metadata and the
paired Broker source described in the packaging guide. A successful local Linux
build does not qualify Windows, macOS or a private distribution's deployment.

## Global directory and environment names

The brand controls the default global directory and environment prefix. A build
with `GRAPHIT_BUILD_BRAND=acme-code` uses `~/.acme-code/` and the
environment prefix `ACME_CODE_*`. The
environment-only `<PREFIX>_GLOBAL_DIR` can relocate that directory before config
resolution begins.

Global state includes configuration, global rules, local authoritative stores, compiled AST/wiki
stores, runtime payloads, models, daemon metadata, managed file artifacts, and the bounded Hub cache
at `~/.<brand>/hub/cache/`. The cache is isolated by Hub and trusted subject, is safe to remove, and
is neither an ACL authority nor a complete registry mirror. Checkout-local files
are sources, identity, configuration and rule/query overrides; module stores,
caches and logs use the global PROJECT namespace. See
[Storage Layout](../architecture/storage_layout.md).

## Rust namespace and path context

`Brand`, `InstallationPaths` and `ProjectPaths` are immutable owned contexts,
`Clone`/`Send`/`Sync`, consumed by the implemented identity, root-discovery,
version-selector, Gitignore and configuration helpers. They do not read mutable global branding,
environment or working-directory state themselves. The caller/build supplies
the brand and captures bootstrap inputs before normal configuration.

`Brand::new` trims and ASCII-lowercases a 1–64 byte key with alphanumeric edges
and only alphanumeric/hyphens inside. Underscores are rejected to prevent
collisions with the hyphen-to-underscore environment derivation. Git control
keys `git`, `gitignore`, `gitattributes` and `gitmodules`, and Windows device
names, are reserved before filesystem operations.

| Derivation for `acme-code` | Value |
| --- | --- |
| Brand directory | `.acme-code` |
| Environment prefix | `ACME_CODE` |
| Binary / Windows binary | `acme-code` / `acme-code.exe` |
| MCP server for `task` | `acme-code-task-mcp` |
| Skill for `task` | `acme-code-task` |

Environment suffixes and module/tool names are validated by their respective
derivation APIs. `InstallationPaths::resolve(brand, start, home, global_override)`
canonicalizes the existing starting directory. A nonblank override wins: an
absolute path is kept, a relative path is bound once to that captured start.
Without one, an absolute home is required and the default is `home/.<key>`.
Blank overrides count as absent; non-Unicode OS paths are rejected explicitly.
Both the supplied directory and its canonical target must be Unicode. Resolution
creates no state, has no global-to-project fallback and does not drift after
the process changes directory.

Derived path components are portable segments of at most 128 bytes: no
empty/dot/traversal, absolute/prefix, slash/backslash, controls, Windows-invalid
characters, trailing dot/space or device names. Device exclusions include
COM/LPT with digits 1–9 or superscript ¹/²/³, including with extensions. Explicit
root paths keep OS semantics; these segment rules apply to derived components.
A distribution runtime version is a component distinct from a Git branch.

`ProjectPaths::at(root, &installation)` binds a canonical project root and
rejects the canonical installation/global root or aliases. `discover` checks
that guard at its canonical entry before ascending, then applies brand-specific
nearest identity/Git boundaries and checks the found root too. It creates no state.
The same context derives `.<key>/project.json` and its adjacent
`.<key>/.project.json.lock`. `project_runtime(captured_project_id, parts)` derives
`<global>/projects/<PROJECT_ID>/<parts>` without reading or creating identity.
Checkouts of the same PROJECT share storage; Lance branches select versions.
Identity stores consume `&ProjectPaths`; version
selection consumes the resolved context without repeating filesystem discovery.
Pins compare canonical checkout root, global root and brand key, isolating
different installations and brands in one checkout.

See [Storage Layout](../architecture/storage_layout.md#authoritative-local-data)
for committed identity/configuration and global machine-owned storage.

The historical Linux 90-entry brand/path gate passed, including ten path-context tests, real child
process directory changes, reserved-name checks, two identities/pins and
cross-brand locking/real Git. Final Linux gate and Windows GNU locked all-targets
compilation passed after reserved names, entry guards and the shared-canonical
refactor. Compilation is not Windows runtime evidence. Runtime
macOS/Windows and remote CI were not qualified by that library gate. Those
historical results do not qualify the current product or a private deployment.
The build inputs above describe the maintained App consumer. Bucket, region and
endpoint are never client branding. See
[product architecture](../development/product-architecture.md).

### Rust configuration names and captured inputs

The implemented config library consumes the same immutable contexts: global
config lives at the installation root, project config under `.<key>/config.json`.
Neither a getter nor a snapshot reads changing environment/CWD state. Bootstrap
location is resolved before constructing these config layers. Product consumers
use the captured brand; environment names follow the Rust derivation below.

`ConfigKey` derives environment suffixes with dots replaced by double underscores
and single underscores unchanged. `acme-code` / `modules.ast` derives
`ACME_CODE_MODULES__AST`. Keys are canonical, bounded and reject double
underscores. This gives unique names by key within one brand, but not for every
brand/key pair: `acme` / `code_modules.ast` derives that same name. Capture takes
an explicit brand and requested-key list and ignores unrelated variables. The
namespace is not an authorization boundary. Configuration authentication keys
and bootstrap aliases are rejected, rather than merged into another brand's
provider/account authority.

Global config uses `.config.json.lock`. A Project stores identity, root metadata,
root `githooks` and `settings` together in `.<brand>/project.json`; its writers
share `.project.json.lock`. Contexts addressing the same physical document use
the same lock, while runtime/grammar patterns and marker pairs remain brand-specific.
The current Linux config gate passed 109 entries and Windows GNU locked
all-targets compilation passed; macOS/Windows runtime and remote CI remain
unexecuted. See [Config Module](../specs/config_module.md#rust-configuration-contract)
for scope/secret/type rules and
[product architecture](../development/product-architecture.md)
for measured costs and remaining consumers.

## Setting up private collaboration ecosystems

Deploy [Graphit Broker](https://github.com/graphit-labs/graphit-broker) in front of a private AWS S3
bucket or an S3-compatible service such as MinIO. Configure bucket, region, endpoint, base prefix,
permanent STS caller key, and assumable role only on the Broker; manage normalized resource grants
through its administration UI/API and SQL database. Hub v2 uses logical keys rooted below immutable
project ULIDs, while remote Memory and Task use their own authoritative LanceDB prefixes.

For a single-user or workload deployment, configure the branded equivalents of
the active named account profile. A multi-user service should
bind the authenticated subject to each request instead; it must not accept these values from an API
payload or project file.

```bash
graphit setup
```

Setup captures the native agent and optionally initializes/maintains a PROJECT. It does not
silently provision a chat account or download every model. Configure named providers/accounts
through their dedicated commands. A Broker provider accepts Broker or disabled inference modes;
a Local provider can select its services independently. The private Auth store retains account
credentials; Broker-issued temporary S3 sessions remain only in process memory. Standalone Local
profiles may retain their explicitly configured S3 credentials. See [Authentication](authentication.md)
and [S3](s3-and-ui-network.md) for exact boundaries.

### Storage isolation

Choose prefixes, buckets, accounts, or broker instances on the broker side. The broker may route
organizations and projects dynamically without updating or re-logging client providers.

## UI network hardening

The shared Source/UI listener uses `mcp.host` and `mcp.port` (loopback and ephemeral port by
default). The current Browser requires trusted Global `task.browser` with Broker providers and
PROJECT authorization endpoint. It uses opaque session cookies and server-memory credentials;
`ui.auth.enabled`, `ui.host` and `ui.auth.public_url` are not current runtime controls.

```sh
graphit config --global set mcp.host 127.0.0.1
graphit config --global set mcp.port 0
```

For an explicitly exposed service, configure the intended listener, HTTPS reverse proxy and exact
`task.browser.public_origin` with the real Broker/PROJECT endpoint. Do not infer origin from
forwarding headers or replace current PROJECT authorization with CORS. Native Broker browser
registration/login must be configured independently. See [Browser authentication](authentication.md#web-ui-login),
[all fields/defaults](../reference/configuration-keys.md#browser-object) and [container deployment](container.md).

## Private model and API policy

The local embedding engine does not require an LLM API key. Cloud storage credentials remain in
the broker and are distinct from direct model-provider API keys. A private distribution can keep
embeddings local under a Local provider; an active Broker provider routes each enabled
embedding/rerank service exclusively through Broker or explicitly disables it. Prompt-completion integrations remain independent and
may be disabled, proxied, or explicitly configured.

## Air-gapped deployments

The Rust package contains `graphit`, `graphit-model-server` and
`graphit-runtime`; model artifacts are provisioned separately. Install the
selected model's complete files and runtime configuration before using an
offline image. Local GGUF slots require the matching verified llama.cpp runtime;
the default EmbeddingGemma2 GGUF backend uses its own runtime. Follow
[AI models and providers](ai_models.md) for the actual model selection and
[native runtime packaging](../development/native-runtime-packaging.md) for
library provenance and package contents. Also provide:

- the complete Rust product package for every target platform;
- any required native libraries and grammar packages;
- an internal Broker with STS-capable S3 storage, direct provider S3 configuration, or no S3 for local-only operation;
- firewall/DNS rules that prevent unintended egress.

Validate the branded binary in a clean environment before release: run setup,
initialize a sample project, perform an S3 publish/install cycle when remote mode
is enabled, and verify UI access through the intended network boundary.
