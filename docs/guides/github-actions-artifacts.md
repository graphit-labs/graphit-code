---
title: "GitHub Actions Artifacts"
description: "Build and publish branch- and tag-scoped AST and knowledge artifacts to a production Graphit Hub without interactive setup."
content-type: guide
audience: operators
keywords:
  - GitHub Actions
  - CI
  - Hub
  - S3
  - LanceDB
  - embeddings
related:
  - "docs/guides/container.md"
  - "docs/guides/configuration.md"
  - "docs/guides/cli_reference.md"
  - "docs/guides/user_manual.md"
  - "docs/architecture/storage_layout.md"
  - "docs/specs/hub_collaboration.md"
  - "docs/specs/hub-s3-object-layout.md"
---

# GitHub Actions Artifacts

This workflow rebuilds a repository's AST and documentation indexes after pushes to `main` or
`master`, and after every tag push. It waits for heavy processing and embeddings, then publishes both
compiled contexts to a production S3-backed Hub. It installs the latest Graphit release on every run
and needs no terminal input. S3 and a remote embedding provider are mandatory for this workflow;
local embeddings are rejected before setup starts.

The repository must already contain a committed `graphit.lock.json` with a non-empty `project.id`.
That identity scopes the published S3 prefixes. CI must not run `graphit init`, because generating a
new identity on an ephemeral runner would create a different publishing project on every run.

The two published channel types have different storage contracts:

| Push | Published version | LanceDB behavior |
|---|---|---|
| Branch `main` | `branch/main` | Advances a mutable branch lineage and records the clean Git commit with exact table versions and native tags. |
| Branch `feature/api` | `branch/feature/api` | Preserves the complete slash-separated branch name in an isolated mutable lineage. |
| Tag `v2.0.0` | `tag/v2.0.0` | Publishes a self-contained snapshot whose tables contain only the current data version. |

The AST graph is not layered: LadybugDB/Icebug is derived from source and rebuilt on the runner.
Only the LanceDB search and knowledge tables use a remote base with a local overlay.

## Production environment

Create a GitHub environment named `graphit-production`. Keep its deployment branch/tag rules as
narrow as your production policy requires; do not add required reviewers if publication must remain
fully unattended.

Add these environment secrets:

| Secret | Required when | Purpose |
|---|---|---|
| `GRAPHIT_HUB_ACCESS_KEY_ID` | yes | S3 access key ID |
| `GRAPHIT_HUB_SECRET_ACCESS_KEY` | yes | S3 secret access key |
| `GRAPHIT_AI_EMBEDDING_API_KEY` | yes | Remote embedding provider credential |
| `GRAPHIT_AI_RERANK_API_KEY` | the rerank provider requires authentication | Rerank API credential |

Add these environment variables:

| Variable | Required | Example |
|---|---:|---|
| `GRAPHIT_HUB_BUCKET` | yes | `graphit-production` |
| `GRAPHIT_HUB_REGION` | yes | `us-east-1` |
| `GRAPHIT_HUB_ENDPOINT` | only for a custom S3-compatible service | `https://s3.example.com` |
| `GRAPHIT_HUB_PREFIX` | no | `graphit` |
| `GRAPHIT_HUB_SUBJECT_USER` | yes unless the runner transport binds a subject | `ci-publisher` |
| `GRAPHIT_HUB_SUBJECT_TEAMS` | no | `release-engineering,platform` |
| `GRAPHIT_AST_ARTIFACT_BASENAME` | yes | `payments-ast` |
| `GRAPHIT_KNOWLEDGE_ARTIFACT_BASENAME` | yes | `payments-docs` |
| `GRAPHIT_TAG_BASE_BRANCH` | no | Branch lineage used by detached tag builds; defaults to the repository default branch |
| `GRAPHIT_AI_EMBEDDING_PROVIDER` | yes | `openai`, `openai-compatible`, `cohere`, `voyage`, or `google`; never `local` |
| `GRAPHIT_AI_EMBEDDING_MODEL` | yes | `text-embedding-3-large` |
| `GRAPHIT_AI_EMBEDDING_BASE_URL` | `openai-compatible` only | `https://llm.example.com/v1` |
| `GRAPHIT_AI_EMBEDDING_DIMENSIONS` | model dependent | `3072` |
| `GRAPHIT_AI_RERANK_PROVIDER` | yes | `cohere`, `voyage`, `jina`, or `local` |
| `GRAPHIT_AI_RERANK_MODEL` | provider dependent | provider model name |
| `GRAPHIT_AI_RERANK_BASE_URL` | no | custom rerank endpoint |

The publisher first reads the repository's immutable project ULID from `graphit.lock.json`. Its S3
principal should be limited to `v2/projects/<that-ulid>/`, plus the exact conditional name-registry
operation required to register or rename that project. It must not write global, anonymous,
authenticated, user, or team ACL documents and should not list unrelated project prefixes. Branch publication preserves Lance data
and commit manifests while mirroring the remaining artifact files; tag publication replaces its
compact snapshot exactly. This self-contained example uses S3 credentials as GitHub environment
secrets, but an OIDC workload role with the same prefix restriction is preferred.

## Workflow

Create `.github/workflows/graphit-artifacts.yml`:

```yaml
name: Publish Graphit contexts

on:
  push:
    branches:
      - main
      - master
    tags:
      - "**"

permissions:
  contents: read

# Artifact entry updates for one project must be serialized across repository refs.
concurrency:
  group: graphit-hub-publisher-${{ github.repository }}
  cancel-in-progress: false

jobs:
  publish:
    runs-on: ubuntu-latest
    timeout-minutes: 120
    environment: graphit-production

    env:
      GRAPHIT_GLOBAL_DIR: /tmp/graphit-global-${{ github.run_id }}-${{ github.run_attempt }}
      GRAPHIT_HUB_BUCKET: ${{ vars.GRAPHIT_HUB_BUCKET }}
      GRAPHIT_HUB_REGION: ${{ vars.GRAPHIT_HUB_REGION }}
      GRAPHIT_HUB_ENDPOINT: ${{ vars.GRAPHIT_HUB_ENDPOINT }}
      GRAPHIT_HUB_PREFIX: ${{ vars.GRAPHIT_HUB_PREFIX }}
      GRAPHIT_HUB_ACCESS_KEY_ID: ${{ secrets.GRAPHIT_HUB_ACCESS_KEY_ID }}
      GRAPHIT_HUB_SECRET_ACCESS_KEY: ${{ secrets.GRAPHIT_HUB_SECRET_ACCESS_KEY }}
      GRAPHIT_AI_EMBEDDING_PROVIDER: ${{ vars.GRAPHIT_AI_EMBEDDING_PROVIDER }}
      GRAPHIT_AI_EMBEDDING_MODEL: ${{ vars.GRAPHIT_AI_EMBEDDING_MODEL }}
      GRAPHIT_AI_EMBEDDING_BASE_URL: ${{ vars.GRAPHIT_AI_EMBEDDING_BASE_URL }}
      GRAPHIT_AI_EMBEDDING_DIMENSIONS: ${{ vars.GRAPHIT_AI_EMBEDDING_DIMENSIONS }}
      GRAPHIT_AI_EMBEDDING_API_KEY: ${{ secrets.GRAPHIT_AI_EMBEDDING_API_KEY }}
      GRAPHIT_AI_RERANK_PROVIDER: ${{ vars.GRAPHIT_AI_RERANK_PROVIDER }}
      GRAPHIT_AI_RERANK_MODEL: ${{ vars.GRAPHIT_AI_RERANK_MODEL }}
      GRAPHIT_AI_RERANK_BASE_URL: ${{ vars.GRAPHIT_AI_RERANK_BASE_URL }}
      GRAPHIT_AI_RERANK_API_KEY: ${{ secrets.GRAPHIT_AI_RERANK_API_KEY }}
      GRAPHIT_AST_ARTIFACT_BASENAME: ${{ vars.GRAPHIT_AST_ARTIFACT_BASENAME }}
      GRAPHIT_KNOWLEDGE_ARTIFACT_BASENAME: ${{ vars.GRAPHIT_KNOWLEDGE_ARTIFACT_BASENAME }}
      GRAPHIT_GIT_BASE_BRANCH: ${{ github.ref_type == 'branch' && github.ref_name || vars.GRAPHIT_TAG_BASE_BRANCH || github.event.repository.default_branch }}
      GRAPHIT_IDE: codex
      GRAPHIT_CLI: codex
      GRAPHIT_MODULES_AGENT: "false"
      GRAPHIT_MODULES_DREAM: "false"
      GRAPHIT_MODULES_MEMORY: "false"
      GRAPHIT_MODULES_TASK: "false"
      GRAPHIT_MODULES_AST: "true"
      GRAPHIT_MODULES_KNOWLEDGE: "true"
      GRAPHIT_MODULES_EMBEDDING: "true"
      GRAPHIT_MODULES_DAEMON: "false"
      GRAPHIT_MODULES_SYNC: "false"
      GRAPHIT_MODULES_HOOKS: "false"
      NO_COLOR: "1"

    steps:
      - name: Check out the pushed ref
        uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Validate publishing inputs
        shell: bash
        run: |
          set -euo pipefail
          : "${GRAPHIT_HUB_BUCKET:?Set GRAPHIT_HUB_BUCKET in graphit-production}"
          : "${GRAPHIT_HUB_REGION:?Set GRAPHIT_HUB_REGION in graphit-production}"
          : "${GRAPHIT_HUB_ACCESS_KEY_ID:?Set GRAPHIT_HUB_ACCESS_KEY_ID in graphit-production}"
          : "${GRAPHIT_HUB_SECRET_ACCESS_KEY:?Set GRAPHIT_HUB_SECRET_ACCESS_KEY in graphit-production}"
          : "${GRAPHIT_AST_ARTIFACT_BASENAME:?Set GRAPHIT_AST_ARTIFACT_BASENAME}"
          : "${GRAPHIT_KNOWLEDGE_ARTIFACT_BASENAME:?Set GRAPHIT_KNOWLEDGE_ARTIFACT_BASENAME}"
          : "${GRAPHIT_AI_EMBEDDING_PROVIDER:?Set GRAPHIT_AI_EMBEDDING_PROVIDER}"
          : "${GRAPHIT_AI_EMBEDDING_MODEL:?Set GRAPHIT_AI_EMBEDDING_MODEL}"
          : "${GRAPHIT_AI_EMBEDDING_API_KEY:?Set GRAPHIT_AI_EMBEDDING_API_KEY in graphit-production}"
          : "${GRAPHIT_AI_RERANK_PROVIDER:?Set GRAPHIT_AI_RERANK_PROVIDER}"
          case "${GRAPHIT_AI_EMBEDDING_PROVIDER,,}" in
            local)
              echo "GRAPHIT_AI_EMBEDDING_PROVIDER must be a remote provider; local is not supported in artifact publishing CI" >&2
              exit 1
              ;;
          esac
          python3 - <<'PY'
          import json
          from pathlib import Path

          lock = json.loads(Path("graphit.lock.json").read_text())
          if not lock.get("project", {}).get("id"):
              raise SystemExit("graphit.lock.json must contain project.id")
          PY

      - name: Install the latest Graphit release
        shell: bash
        run: |
          set -euo pipefail
          curl -fsSL \
            https://raw.githubusercontent.com/graphit-labs/graphit-code/main/install.sh \
            -o "${RUNNER_TEMP}/install-graphit.sh"
          sh "${RUNNER_TEMP}/install-graphit.sh" --dir "${RUNNER_TEMP}/graphit-bin"
          echo "${RUNNER_TEMP}/graphit-bin" >> "${GITHUB_PATH}"

      - name: Derive the branch or tag channel
        id: channel
        shell: bash
        run: |
          set -euo pipefail
          echo "version=${GITHUB_REF_TYPE}/${GITHUB_REF_NAME}" >> "${GITHUB_OUTPUT}"

      - name: Configure Graphit without prompts
        shell: bash
        run: |
          set -euo pipefail
          graphit setup \
            --username "${GRAPHIT_HUB_SUBJECT_USER}" \
            --hub-bucket "${GRAPHIT_HUB_BUCKET}" \
            --hub-region "${GRAPHIT_HUB_REGION:-}" \
            --hub-endpoint "${GRAPHIT_HUB_ENDPOINT:-}" \
            --hub-access-key-id "" \
            --hub-secret-access-key "" \
            --ide "${GRAPHIT_IDE}" \
            --cli "${GRAPHIT_CLI}" \
            --embedding-provider "${GRAPHIT_AI_EMBEDDING_PROVIDER}" \
            --embedding-model "${GRAPHIT_AI_EMBEDDING_MODEL:-}" \
            --embedding-base-url "${GRAPHIT_AI_EMBEDDING_BASE_URL:-}" \
            --embedding-api-key "" \
            --rerank-provider "${GRAPHIT_AI_RERANK_PROVIDER}" \
            --rerank-model "${GRAPHIT_AI_RERANK_MODEL:-}" \
            --rerank-api-key ""

      - name: Build indexes, heavy data, and embeddings
        shell: bash
        run: |
          set -euo pipefail
          graphit sync --no-background
          graphit ast embed
          graphit wiki embed --wiki project

      - name: Publish production contexts
        shell: bash
        env:
          AST_ID: ${{ env.GRAPHIT_AST_ARTIFACT_BASENAME }}
          KNOWLEDGE_ID: ${{ env.GRAPHIT_KNOWLEDGE_ARTIFACT_BASENAME }}
          ARTIFACT_VERSION: ${{ steps.channel.outputs.version }}
        run: |
          set -euo pipefail
          graphit hub submit "${AST_ID}" . \
            --type ast \
            --version "${ARTIFACT_VERSION}" \
            --name "AST for ${GITHUB_REF_TYPE} ${GITHUB_REF_NAME}"
          graphit hub submit "${KNOWLEDGE_ID}" . \
            --type knowledge \
            --version "${ARTIFACT_VERSION}" \
            --name "Knowledge for ${GITHUB_REF_TYPE} ${GITHUB_REF_NAME}"
          graphit hub show "${AST_ID}" --type ast
          graphit hub show "${KNOWLEDGE_ID}" --type knowledge
          {
            echo "### Published Graphit contexts"
            echo "- AST: \`${AST_ID}@${ARTIFACT_VERSION}\`"
            echo "- Knowledge: \`${KNOWLEDGE_ID}@${ARTIFACT_VERSION}\`"
          } >> "${GITHUB_STEP_SUMMARY}"
```

The default branch filter intentionally publishes only `main` and `master`. Add an exact branch or
pattern under `push.branches` when another branch channel, such as `feature/api`, should publish
automatically. Tag pushes remain enabled independently.

The empty credential flags are deliberate. Supplying every reachable setup flag prevents prompts;
empty secret flags prevent credentials from being persisted in the ephemeral global configuration.
The same process still resolves `GRAPHIT_HUB_ACCESS_KEY_ID`, `GRAPHIT_HUB_SECRET_ACCESS_KEY`, and
the provider API keys directly from its environment during verification, sync, and publication.

`graphit sync --no-background` runs both the normal phase and the heavy phase synchronously. The two
explicit embedding commands are intentional completion barriers: they are incremental after sync,
but make the job fail if either AST or project-wiki embeddings are unavailable or incomplete. A
fresh `GRAPHIT_GLOBAL_DIR` prevents an ephemeral runner from publishing stale local state. Before
parsing, `graphit sync` inspects the Git repository, branch, HEAD, ancestry, and dirty state. If the
Hub contains a compatible snapshot for the exact commit or its nearest published ancestor, Graphit
shallow-clones each Lance table into that fresh filesystem directory. Inherited fragments remain on
S3, while parsing changes and new embeddings are written locally. Ladybug/Icebug is rebuilt normally
because its graph is derived from source.

Git is an optimization and provenance boundary, not a requirement for ordinary sync. Outside a Git
repository, Graphit skips branch discovery and remote shallow-clone hydration, then builds and
updates the same filesystem-local LanceDB and LadybugDB stores normally. A non-Git project may also
publish a new `branch/...` name as a mutable exact snapshot, but that channel has no commit history
or ancestor reuse. It cannot overwrite a branch that already has Git-backed Lance history; publish
from the repository that owns that history or choose a different branch name.

The Hub accepts named versions, including Git branch paths with `/`. Prefixing the name with
`branch/` or `tag/` distinguishes a branch and tag with the same display name. The logical version
is kept unchanged in the registry and encoded internally as one collision-free path segment, so
`branch/feature/api` cannot overlap the prefix for `branch/feature` in S3 or in a local mount cache.

Publishing a `tag/...` version automatically creates a release snapshot in temporary staging. Every
LanceDB table is compacted, all superseded MVCC versions are pruned, and publication fails unless
exactly one current table version remains. The current rows and search indexes are preserved. This
does not mutate the runner's working database and does not require another workflow command.

`GRAPHIT_GIT_BASE_BRANCH` gives a detached tag checkout a branch lineage to reuse. Full checkout
history is required so Graphit can walk from the tagged commit to the nearest published ancestor.
For branch pushes the variable resolves to the branch name itself, including names containing `/`.
For tags it uses `GRAPHIT_TAG_BASE_BRANCH` when configured and otherwise uses the repository default
branch. Set that variable when release tags normally originate from another branch.

The validation step intentionally requires S3 credentials and a remote embedding provider, model,
and API key. Do not change the provider to `local` in this publishing job: a hosted runner must build
the production embeddings through the configured provider so the generated artifact is independent
of runner-local model state.

## Update and cleanup semantics

A Hub artifact is mutable registry state. For `branch/...`, every clean Git commit is recorded in a
small manifest and protected by a native Lance tag on every table. Publication advances the branch
by applying one exact table snapshot. The Graphit release version is audit metadata only. Cache
compatibility depends on the artifact format plus the embedding provider, model, and dimensions, so
a CI job that always installs `latest` does not invalidate compatible embeddings. The local shallow
clone reuses inherited embeddings without recomputing them; remote branch publication may create new
Lance fragments while preserving the tagged versions needed by commit history.

All remote keys for the publication remain below
`v2/projects/<project-ulid>/artifacts/<type>/<artifact-id>/<version>/`, and the per-project registry
entry is written last. The globally unique project name is a separate conditional reservation;
changing it does not move branch history or artifact data.

The local project still points only at its filesystem LanceDB. Sync reads inherited fragments from
S3 through the shallow clone, but it never turns the project store into an S3 store and never writes
the Hub during parsing. Only `hub submit` updates the authoritative remote branch. Publishing is
rejected for a dirty worktree or for a branch name that differs from `--version branch/...`, because
otherwise the remote snapshot could not be attributed to one Git commit.

Enabling S3 after local indexing does not replace or merge an initialized local store. The local
tables remain authoritative and sync continues incrementally from them. The first `hub submit` to an
empty branch prefix seeds S3 from that completed local snapshot. If both sides were populated
independently, Graphit does not attempt a row-level two-way merge: publishing advances the remote
branch to the local snapshot, while starting with an empty local store allows compatible remote
history to become the shallow-clone base. Choose one authority before publishing.

Changing only the S3 configuration does not invalidate embeddings. Changing the embedding provider,
model, or dimensions is a separate semantic migration. A fresh CI workspace is safe: the
compatibility fingerprint rejects old remote vectors and the job computes new ones. Do not reuse and
publish an already-populated local store under a different embedding identity. Build it in a fresh
`GRAPHIT_GLOBAL_DIR` first so every vector is regenerated before the new snapshot is published.

Consumers must run `graphit hub update` and close/reopen a mounted context after publication;
advancing a known branch prefix is not an atomic cutover for a reader that already has it open. When
live readers require an immutable cutover, publish a numeric version or a `tag/...` snapshot and
move consumers after the registry pointer is written.

Graphit's normal maintenance does not delete Hub branch history. It compacts project-local indexes
and skips remote mounted stores. Branch commit tags protect every source version that a future sync
may shallow-clone, so do not run external Lance pruning or an S3 lifecycle rule against a live branch
prefix. Removing those manifests or fragments can orphan existing shallow clones. Obsolete branch
history needs an explicit retention operation that first proves no published tag or clone depends on
it; Graphit does not perform that cleanup automatically.

Tag snapshots are stricter than normal local maintenance: the staged LanceDB contains no edit or
time-travel history. Republishing the same tag mirrors that compact snapshot over the tag's prefix,
so stale manifests and data files from its previous publication are deleted. Other branch and tag
prefixes are independent and are not touched; remove an obsolete tag artifact separately only when
your retention policy no longer needs it.

## Native build boundary

The workflow installs the latest released Graphit binary and does not clone LanceDB source. The
companion `graphit-code-libs` repository keeps the source build pinned to one exact `lancedb-go`
main commit and owns the minimal binding patch, which adds the missing shallow-clone bridge and
compatibility changes. Git ancestry, fingerprints, Hub manifests, publication, and hydration stay
in Graphit's own packages. The native build stamp includes both the upstream commit and patch hash,
so an older cached or machine-global library is not reused for this feature.

Install the branch or tag by its direct named version, then run `graphit hub update` after a new push
to refresh the same channel:

```bash
graphit hub install payments-ast@branch/main --type ast
graphit hub update payments-ast --type ast
```
