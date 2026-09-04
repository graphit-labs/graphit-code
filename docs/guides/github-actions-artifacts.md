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
  - "docs/specs/hub_collaboration.md"
  - "docs/specs/hub-s3-object-layout.md"
---

# GitHub Actions Artifacts

This workflow rebuilds a repository's AST and documentation indexes after every branch or tag push,
waits for heavy processing and embeddings, and publishes both compiled contexts to a production
S3-backed Hub. It installs the latest Graphit release on every run and needs no terminal input.
S3 and a remote embedding provider are mandatory for this workflow; local embeddings are rejected
before setup starts.

The repository must already contain a committed `graphit.lock.json` with a non-empty `project.id`.
That identity scopes the published S3 prefixes. CI must not run `graphit init`, because generating a
new identity on an ephemeral runner would create a different publishing project on every run.

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
| `GRAPHIT_AST_ARTIFACT_BASENAME` | yes | `payments-ast` |
| `GRAPHIT_KNOWLEDGE_ARTIFACT_BASENAME` | yes | `payments-docs` |
| `GRAPHIT_AI_EMBEDDING_PROVIDER` | yes | `openai`, `openai-compatible`, `cohere`, `voyage`, or `google`; never `local` |
| `GRAPHIT_AI_EMBEDDING_MODEL` | yes | `text-embedding-3-large` |
| `GRAPHIT_AI_EMBEDDING_BASE_URL` | `openai-compatible` only | `https://llm.example.com/v1` |
| `GRAPHIT_AI_EMBEDDING_DIMENSIONS` | model dependent | `3072` |
| `GRAPHIT_AI_RERANK_PROVIDER` | yes | `cohere`, `voyage`, `jina`, or `local` |
| `GRAPHIT_AI_RERANK_MODEL` | provider dependent | provider model name |
| `GRAPHIT_AI_RERANK_BASE_URL` | no | custom rerank endpoint |

The S3 principal needs list, read, write, and delete permission under `GRAPHIT_HUB_PREFIX`. Delete is
required because republishing a prefix is an exact mirror: objects no longer present in the locally
staged LanceDB are removed after the new files upload successfully. This self-contained example
requires the S3 access key and secret as GitHub environment secrets.

## Workflow

Create `.github/workflows/graphit-artifacts.yml`:

```yaml
name: Publish Graphit contexts

on:
  push:
    branches:
      - "**"
    tags:
      - "**"

permissions:
  contents: read

# Registry entry updates are last-writer-wins. Serialize every ref from this repository.
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

`graphit sync --no-background` runs both the normal phase and the heavy phase synchronously. The two
explicit embedding commands are intentional completion barriers: they are incremental after sync,
but make the job fail if either AST or project-wiki embeddings are unavailable or incomplete. A
fresh `GRAPHIT_GLOBAL_DIR` also prevents an ephemeral runner from publishing a stale local index.

The Hub accepts named versions, including Git branch paths with `/`. Prefixing the name with
`branch/` or `tag/` distinguishes a branch and tag with the same display name. The logical version
is kept unchanged in the registry and encoded internally as one collision-free path segment, so
`branch/feature/api` cannot overlap the prefix for `branch/feature` in S3 or in a local mount cache.

Publishing a `tag/...` version automatically creates a release snapshot in temporary staging. Every
LanceDB table is compacted, all superseded MVCC versions are pruned, and publication fails unless
exactly one current table version remains. The current rows and search indexes are preserved. This
does not mutate the runner's working database and does not require another workflow command.

The validation step intentionally requires S3 credentials and a remote embedding provider, model,
and API key. Do not change the provider to `local` in this publishing job: a hosted runner must build
the production embeddings through the configured provider so the generated artifact is independent
of runner-local model state.

## Update and cleanup semantics

A Hub artifact is mutable registry state. This workflow republishes the named branch/tag version,
updates its content hash, and mirrors the new directory over the existing prefix with
last-writer-wins semantics. Consumers must run `graphit hub update` and close/reopen a mounted
context after publication; replacing a known version is not an atomic cutover for a reader that
already has that S3 prefix open. When live readers require snapshot isolation, publish a new numeric
version and move consumers to it after the registry pointer is written.

Graphit's LanceDB maintenance does not delete Hub data. It compacts local indexes and prunes only
superseded local Lance versions after their retention period; remote mounted stores are skipped. Hub
artifact versions remain in S3 and in the registry until explicitly retracted. Do not apply an S3
lifecycle rule that deletes published version prefixes while registry entries still reference them,
because installation must treat such a pointer as an integrity error.

Tag snapshots are stricter than normal local maintenance: the staged LanceDB contains no edit or
time-travel history. Republishing the same tag mirrors that compact snapshot over the tag's prefix,
so stale manifests and data files from its previous publication are deleted. Other branch and tag
prefixes are independent and are not touched; remove an obsolete tag artifact separately only when
your retention policy no longer needs it.

Install the branch or tag by its direct named version, then run `graphit hub update` after a new push
to refresh the same channel:

```bash
graphit hub install payments-ast@branch/main --type ast
graphit hub update payments-ast --type ast
```
