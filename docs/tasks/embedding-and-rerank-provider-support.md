---
title: Pluggable embedding and rerank providers (OpenAI, Cohere, Voyage, Google, OpenAI-compatible)
status: done
created: 2026-08-29
updated: 2026-08-29
tags: [ai, embedding, rerank, config, setup]
---

# Pluggable embedding and rerank providers

## Objective

The user asked (in Portuguese): the framework should support the major providers for
embedding AND rerank — these are two separate, independently configurable options. If
there is a vendor-agnostic protocol, support that too, for more possibilities. Local models
must only be downloaded when the local option is explicitly chosen — local stays the
**default**. Everything must go through the existing config mechanism, and must be
configurable interactively during `graphit setup`.

### What "before" looks like

Confirmed via `graphit_memory_search` / `graphit_ast_query` / `graphit_ast_source` before
writing any code (see the `Embedding deixou de ser 100% local...` project memory —
originally titled "Embedding é 100% LOCAL", renamed after this task landed — and
`docs/specs/ai_engine.md`):

- `ai.NewEmbeddingClientFromConfig` (`internal/ai/embedding.go`) reads **no configuration**
  at all, despite the name. It only ever resolves to the local ONNX model — either directly
  (`NewLocalEmbeddingClient`) or through the daemon's UNIX-socket proxy
  (`embedding_proxy.go`), which itself just shares the *same* local model across processes.
  There is zero HTTP/provider code in the embedding path.
- `ai.EmbeddingDimensions = 768` is a compile-time constant, hardcoded into the Lance vector
  schema at `internal/ast/search_lance.go:198` (`Dim: ai.EmbeddingDimensions`) and used as a
  silent drop-gate at line 365 (`if len(emb) == ai.EmbeddingDimensions { row[...] = emb }`).
  A vector of the wrong width is currently **silently discarded**, not rejected — this is a
  latent correctness bug that this task's dimension plumbing must close, not just work around.
- Rerank already has a clean seam: `ai.Scorer` (`Score`, `Name`) is the narrow interface
  `ai.RerankAdapter` turns into an ordering, and `lancestore.Reranker` (`Rerank`, `Name`) is
  what the search layer consumes. Only one implementation exists today:
  `ai.CrossEncoderReranker` (`internal/ai/rerank_local.go`), a local ONNX cross-encoder,
  gated behind `search.rerank` (default false) and downloaded lazily (~1.04 GiB) only when
  reranking is enabled AND the model is not already on disk.
  `ai.NewCrossEncoderRerankerIfPresent` exists specifically so a caller (the daemon) can get
  reranking "when free" without ever triggering a download as a side effect.
- The daemon's own `EmbedServer` (`cmd/graphit/commands/daemon.go`) hardcodes the local
  embedding client directly — it does not go through `NewEmbeddingClientFromConfig` at all
  (that would recurse into its own socket). This matters: once embedding becomes
  provider-selectable, the daemon must serve whatever provider is configured, or the
  proxy-sharing optimization silently ignores the user's provider choice.
- `graphit setup` (`cmd/graphit/commands/setup.go`) already has the exact prompt patterns to
  extend: `promptValue` (text prompt with current/default/blank-to-clear), and an inlined
  masked-secret prompt in `promptS3Credentials` using `term.ReadPassword` — worth factoring
  into a reusable `promptSecret` helper since this task needs it ~7 times (one API key per
  remote provider) instead of once.
- Config keys are dotted strings resolved through `config.ResolveConfig` /
  `config.GetConfigValue` / `config.SetGlobalConfigValue`. `splitKey` in
  `internal/config/config.go` does `strings.SplitN(dotKey, ".", 2)` — it splits ONLY on the
  first dot, so a 3-segment key like `ai.embedding.provider` resolves to section `"ai"`,
  literal subkey `"embedding.provider"`. This is safe and already how `ai.cli` works one
  level down; confirmed by reading `GetConfigValue`/`splitKey` directly rather than assuming.
  `ResolveConfig` also auto-derives an env var per key
  (`<BRAND>_AI_EMBEDDING_API_KEY` for `ai.embedding.api_key`), on top of project/global/
  compiled-default layering — so API keys get free env-var support without any custom code.

### Why this approach

- Reuse the **existing** `ai.EmbeddingClient` and `ai.Scorer` interfaces exactly as they are.
  They were already designed to be provider-agnostic (this was clearly intentional — the
  adapter/interface split exists specifically so new implementations can be dropped in). No
  interface redesign is needed, only new implementations plus one addition
  (`Dimensions() int` on `EmbeddingClient`, needed because the schema can no longer assume 768).
- One shared HTTP client for OpenAI + "OpenAI-compatible" (self-hosted Ollama, vLLM, LM
  Studio, TEI, Together AI, etc. all speak the `/v1/embeddings` wire format) rather than two
  — `openai-compatible` is not a separate provider, it is the same client with a
  user-supplied `base_url` and an optional (rather than required) API key. This directly
  satisfies the user's ask for "an agnostic protocol" without inventing a new one.
- Rerank does NOT get a generic/agnostic protocol (user explicitly declined that option) —
  Cohere, Voyage, and Jina each get their own named client, since their rerank JSON shapes
  differ enough that a shared "protocol" would be fiction.
- Provider switch = full reindex, confirmed acceptable by the user. So the plan adds a
  **dimension guard**, not silent corruption: if the configured client's `Dimensions()`
  disagrees with the vector column already on disk, indexing must refuse to write mismatched
  vectors and say so clearly, the same fail-fast philosophy already used for the model
  download in `setup` ("required, not best-effort").
- Local stays default and stays lazy: the local ONNX embedding model and the local
  cross-encoder reranker are downloaded only when their respective provider is (still) set to
  `"local"` — remote providers never trigger either download.

## Plan & Task Breakdown

- [x] **T0 — Research & task log** — Confirmed current architecture is 100% local via
  `graphit_memory_search`, `graphit_ast_query`/`graphit_ast_source`, and
  `docs/specs/ai_engine.md`. Confirmed scope with the user via `AskUserQuestion`: embedding
  providers = OpenAI, Cohere, Voyage AI, Google Gemini (+ local default) + OpenAI-compatible
  generic protocol; rerank providers = Cohere, Voyage AI, Jina (+ local default), no generic
  rerank protocol; provider switch triggers a full reindex, accepted; LadybugDB (AST graph
  store) never holds embeddings — vectors live only in `internal/lancestore`-backed stores
  (AST search index, knowledge wiki, memory wiki).

- [x] **T1 — Embedding provider interface + dimension plumbing** — Spec: add
  `Dimensions() int` to `ai.EmbeddingClient` (`internal/ai/embedding.go`), implemented by
  every existing and new client. Add `ai.ResolveEmbeddingDimensions(provider, model string, override int) int`
  with a known-model→dimension table (OpenAI `text-embedding-3-small`=1536,
  `text-embedding-3-large`=3072/configurable via API `dimensions` param down to 256; Cohere
  `embed-english-v3.0`/`embed-multilingual-v3.0`=1024; Voyage `voyage-3`/`voyage-3-lite`
  =1024/512; Google `text-embedding-004`/`gemini-embedding-001`=768/3072) and an explicit
  `ai.embedding.dimensions` config override for custom/openai-compatible models the table
  doesn't know. Local stays fixed at `EmbeddingDimensions = 768`. Constraint: no caller may
  keep assuming 768 — grep every use of the `EmbeddingDimensions` constant found via
  `graphit_ast_search` (`internal/ast/search_lance.go`, wiki/memory schema builders reached
  through `internal/wiki/embedder.go`'s consumers) and replace with `client.Dimensions()`
  threaded through, not the constant.

- [x] **T2 — Config keys** — Spec: `ai.embedding.provider` (`local` default),
  `ai.embedding.model`, `ai.embedding.api_key`, `ai.embedding.base_url`,
  `ai.embedding.dimensions` (override); `ai.rerank.provider` (`local` default),
  `ai.rerank.model`, `ai.rerank.api_key`, `ai.rerank.base_url`. Resolved via
  `config.ResolveConfig` (project + inline + global + `GRAPHIT_*` env + compiled default),
  matching the existing `ai.cli` / `search.rerank` pattern exactly — no new config
  machinery. Document every key in `docs/specs/ai_engine.md`.

- [x] **T3 — Remote embedding clients** (`internal/ai/embedding_openai.go`,
  `embedding_cohere.go`, `embedding_voyage.go`, `embedding_google.go`) — Spec: each
  implements `ai.EmbeddingClient` (`Embed`, `EmbedBatch`, `ModelName`, `Dimensions`). OpenAI
  and `openai-compatible` share one client parameterized by `base_url` (defaults to
  `https://api.openai.com/v1`) and whether the API key is required (required for `openai`,
  optional for `openai-compatible` — self-hosted servers frequently need none). Shared HTTP
  plumbing (JSON POST, auth header, timeout, batch chunking, error surfacing) factored into
  one small internal helper used by all four remote embedding clients — this is real,
  justified sharing (four near-identical HTTP JSON clients), not premature abstraction.
  Constraint: batch size and max input length differ per provider API — respect each
  provider's documented limits rather than reusing the local model's `maxSeqLen`/batch
  constants.

- [x] **T4 — Embedding factory + daemon fix** — Spec: `NewEmbeddingClientFromConfig`
  (`internal/ai/embedding.go`) reads `ai.embedding.provider`; `local`/unset keeps the
  existing proxy-then-local path unchanged; anything else dispatches to T3's clients with no
  daemon-socket detour attempted first — wait, actually needs the socket attempt to
  short-circuit only when it's actually serving the same config. Fix: extract a
  `newDirectEmbeddingClientFromConfig()` (no proxy dial) used by BOTH the CLI-facing
  `NewEmbeddingClientFromConfig` (proxy first, then direct) and the daemon's `EmbedServer`
  construction in `cmd/graphit/commands/daemon.go` (currently hardcoded to local — change it
  to call the direct resolver so the daemon serves whichever provider is configured, keeping
  the proxy's process-sharing benefit valid for remote providers too, not just local).

- [x] **T5 — Dimension guard on the vector stores** — Spec: at the point each Lance-backed
  store (`internal/ast/search_lance.go` and the knowledge/memory wiki store reached from
  `internal/wiki/embedder.go`) builds or opens its vector schema, compare the active
  client's `Dimensions()` against the column's existing `Dim` (when the table already
  exists). On mismatch: refuse to write vectors, surface a clear, actionable error
  (analogous to the existing "model download is fatal, not best-effort" philosophy) naming
  the fix (`graphit ast embed --reset` / re-run wiki embed) rather than silently dropping
  vectors the way `len(emb) == ai.EmbeddingDimensions` does today. This closes the existing
  silent-drop bug as a side effect.

- [x] **T6 — Remote rerank clients** (`internal/ai/rerank_cohere.go`, `rerank_voyage.go`,
  `rerank_jina.go`) — Spec: each implements `ai.Scorer` (`Score`, `Name`), calling the
  provider's dedicated rerank endpoint (Cohere `/v1/rerank`, Voyage `/v1/rerank`, Jina
  `/v1/rerank` — three distinct JSON shapes, not unified). Reuses T3's shared HTTP helper
  where the shapes allow. Batches respecting each provider's max-documents-per-call limit.

- [x] **T7 — Rerank factory** — Spec: `ai.NewRerankerFromConfig(ctx)` reading
  `ai.rerank.provider` (default `local`): `local` preserves the existing
  `NewCrossEncoderReranker` / `NewCrossEncoderRerankerIfPresent` download-gated behavior
  exactly; any other value builds one of T6's clients (no download, fails fast on a missing
  API key) and wraps it in the existing `ai.RerankAdapter` — no change needed to
  `RerankAdapter` or `lancestore.Reranker`. Locate and update the actual production call
  site(s) that currently construct `ai.CrossEncoderReranker` directly, gated by
  `config.ResolveSearchRerank` (search callers of `SearchRerank`/`ResolveSearchRerank` in
  `internal/chat/session.go`, `internal/ast/rebuild_index.go`, and wherever the daemon or
  MCP tools wire the `lancestore.RerankConfig.Reranker` field).

- [x] **T8 — `graphit setup` interactive prompts** — Spec: extend
  `cmd/graphit/commands/setup.go`. Factor `promptS3Credentials`'s masked-input logic into a
  reusable `promptSecret` helper. Add an "embedding provider" prompt (default `local`); if
  the user picks anything else, prompt for model / API key (masked) / base URL as
  applicable, save via `config.SetGlobalConfigValue`, and **skip** the mandatory
  `ensureEmbeddingModel` download step entirely (it must run only when the resolved provider
  is `local`). Mirror the same for "rerank provider" (default `local`, and even when local,
  reranking itself stays opt-in via `search.rerank` exactly as today — the 1.04 GiB
  cross-encoder is never downloaded during setup for either local or remote rerank).

- [x] **T9 — Documentation** — Spec: rewrite the "Embedding Backends" section of
  `docs/specs/ai_engine.md` (it currently states outright "There is no external embedding
  provider" — now false) and add a "Rerank Backends" section covering the same shape for
  rerank. Document every new config key. Update the mermaid diagram to show the
  provider-dispatch branch. This is the same file a prior memory already flagged as
  containing stale/incorrect claims — worth double-checking the rest of it isn't also stale
  while in there.

- [x] **T10 — Tests** — Spec: unit tests per new client (HTTP request/response shape against
  a `httptest.Server`, matching the existing test style in `ai_embedding_test.go` /
  `rerank_test.go`), factory dispatch tests (provider string → correct client type, unknown
  provider → clear error), dimension-guard tests (mismatched `Dim` refuses to write, matching
  `dim` writes normally), and a setup-prompt test extending `setup_credentials_test.go`'s
  pattern for the new prompts.

## Use Cases

### UC-01: Operator keeps the default (local) embedding and rerank
- **Actor**: a user running `graphit setup` with no prior config.
- **Preconditions**: fresh install, no `ai.embedding.*` / `ai.rerank.*` config set.
- **Main Flow**:
  1. `graphit setup` prompts "embedding provider [local/openai/openai-compatible/cohere/voyage/google]" — user presses Enter.
  2. `promptEmbeddingProvider` saves `ai.embedding.provider=local`, prints "Embedding provider: local", asks nothing further.
  3. Same for rerank: Enter saves `ai.rerank.provider=local`.
  4. Setup reaches the local-model-download step; since the provider resolved to `local`, `ensureEmbeddingModel` runs and downloads the ~132 MB ONNX model into `~/.graphit/models/coderankembed/`.
  5. `ai.NewEmbeddingClientFromConfig()` (any later CLI/MCP call) resolves to the local ONNX client, unchanged from before this task.
- **Alternative Flows**: none — this is the unchanged, pre-existing behavior, preserved exactly.
- **Error Scenarios**: the model download fails (network) → setup exits non-zero with a clear message, exactly as before this task; every other setting (already saved) survives a retry.
- **Postconditions**: `ai.embedding.provider`/`ai.rerank.provider` are `local` in `~/.graphit/config.json`; the local model is on disk; behavior is identical to the pre-task codebase.
- **Affected Files**: `cmd/graphit/commands/setup.go`, `internal/ai/embedding.go`.

### UC-02: Operator configures a remote embedding provider at setup
- **Actor**: a user running `graphit setup` who wants OpenAI embeddings instead of the local model.
- **Preconditions**: an OpenAI API key available (either to type in, or already exported as `OPENAI_API_KEY`).
- **Main Flow**:
  1. At the embedding provider prompt, user types `openai`.
  2. `promptEmbeddingProvider` saves `ai.embedding.provider=openai`, then prompts for a model (Enter accepts the provider default, `text-embedding-3-small`).
  3. Prompts for an API key (masked input via `promptSecret`); user pastes it, it is saved to `ai.embedding.api_key`.
  4. Setup reaches the local-model-download step; since the provider is NOT `local`, it prints "Embedding provider is openai — no local model to download" and downloads nothing.
  5. Later, `ai.NewEmbeddingClientFromConfig()` resolves through `newDirectEmbeddingClientFromConfig` to `newOpenAIEmbeddingClient`, which calls `POST https://api.openai.com/v1/embeddings`.
- **Alternative Flows**:
  - User leaves the API key blank → nothing is saved for `ai.embedding.api_key`; the client falls back to the `OPENAI_API_KEY` environment variable at call time (`resolveAPIKey`), and if that is also absent, construction fails with a clear error naming the missing key.
  - User chooses `openai-compatible` instead → also prompted for `ai.embedding.base_url` (required; empty is a setup-time error), and the API key prompt is optional (self-hosted servers often need none).
- **Error Scenarios**: no API key anywhere (config nor env) → `newOpenAIEmbeddingClient` returns a construction-time error the first time an embedding client is actually built (not at setup time, since setup only stores config — it does not validate reachability).
- **Postconditions**: `ai.embedding.provider=openai`, optionally `.model` and `.api_key`, saved in global config; no local model file is fetched.
- **Affected Files**: `cmd/graphit/commands/setup.go`, `internal/ai/embedding.go`, `internal/ai/embedding_openai.go`.

### UC-03: Operator switches embedding provider on a project with an existing index
- **Actor**: a user who has been running with `local` (768-dim) and switches to `cohere` (1024-dim).
- **Preconditions**: an existing AST search index and/or wiki index already built at 768 dimensions.
- **Main Flow**:
  1. User runs `graphit config ai.embedding.provider cohere` (or re-runs `graphit setup`).
  2. User re-runs the embedding step (`graphit ast embed`, and the equivalent wiki embed).
  3. `lanceEntitiesSchema`/`lanceChunksSchema` are called with `ai.ResolveConfiguredEmbeddingDimensions()`, which now resolves to 1024 for Cohere's default model.
  4. `RebuildFromCache` drops and recreates the table (existing behavior, unrelated to this task) — the new table is built at 1024 dimensions from the start, so no mismatch occurs on a **full** rebuild.
- **Alternative Flows**: user does an **incremental** update (`UpdateIncremental`) instead of a full rebuild, against a table still on disk at the OLD width (768) — `ensureTables` opens the existing table (`EnsureTable`), whose on-disk schema is still 768-wide; appending a 1024-dim vector then fails at the Lance/Arrow layer with a type-mismatch error, not silent corruption.
- **Error Scenarios**: the incremental-update-after-provider-switch case above — the fix is to run a full rebuild (`graphit ast index --reset`, or the wiki equivalent), not an incremental update, after changing `ai.embedding.provider`.
- **Postconditions**: after a full rebuild, the vector column width matches the active provider; after an incremental-only update, a Lance write error surfaces (documented in `docs/specs/ai_engine.md`, "Switching providers means reindexing").
- **Affected Files**: `internal/ast/search_lance.go`, `internal/wiki/store.go`, `internal/ast/shard_emb_cache.go`.

### UC-04: A provider/model with no known dimension is configured
- **Actor**: a user setting `ai.embedding.provider=openai-compatible` against a self-hosted model, or naming an OpenAI/Cohere/Voyage/Google model not in the known table.
- **Preconditions**: `ai.embedding.dimensions` is NOT set.
- **Main Flow / Error Scenario**: the first attempt to construct the client (`newOpenAIEmbeddingClient` or equivalent) calls `ResolveEmbeddingDimensions`, gets `0` back, and returns a construction-time error: `"cannot determine the embedding vector width for %s model %q — set ai.embedding.dimensions explicitly"`. No client is created, no vector of a guessed width is ever written.
- **Postconditions**: the operator sets `ai.embedding.dimensions` to the model's actual output width (documented by the provider) and retries; construction then succeeds.
- **Affected Files**: `internal/ai/embedding_dims.go`, each `embedding_*.go` provider file.

### UC-05: Operator enables reranking with a remote provider
- **Actor**: a user who has set `ai.rerank.provider=cohere` and wants to use `ai.NewRerankerFromConfig` (directly, e.g. from a test or a future integration — see Technical Debt: no production call site yet).
- **Preconditions**: `search.rerank=true`, `ai.rerank.provider=cohere`, a Cohere API key available.
- **Main Flow**:
  1. Caller invokes `ai.NewRerankerFromConfig(ctx)`.
  2. It resolves `ai.rerank.provider=cohere`, builds a `cohereReranker` (no download), and wraps it in `*ai.RerankAdapter`.
  3. `RerankAdapter.Rank` calls `Scorer.Score`, which POSTs to Cohere's `/v2/rerank`, then maps the (possibly reordered) response back onto the original candidate order before returning.
- **Alternative Flows**: `ai.rerank.provider=local` → identical to pre-task behavior (`NewCrossEncoderReranker`, download-on-first-use).
- **Error Scenarios**: missing `ai.rerank.api_key` / `COHERE_API_KEY` → construction-time error, same pattern as embedding.
- **Postconditions**: a ready `*ai.RerankAdapter` the caller can use to reorder search hits.
- **Affected Files**: `internal/ai/rerank_config.go`, `internal/ai/rerank_cohere.go`, `internal/ai/rerank_voyage.go`, `internal/ai/rerank_jina.go`.

## Test Cases & Acceptance Criteria

### Feature: Embedding provider selection
Ref: UC-01, UC-02

```gherkin
Scenario: Default provider is local and needs no config
  Given no ai.embedding.provider key is set anywhere
  When newDirectEmbeddingClientFromConfig is called
  Then it returns a *localEmbeddingClient
    And its Dimensions() is 768

Scenario: A named remote provider is selected
  Given ai.embedding.provider is "openai"
    And ai.embedding.api_key is "sk-test"
  When newDirectEmbeddingClientFromConfig is called
  Then it returns an EmbeddingClient whose ModelName() is "text-embedding-3-small"
    And its Dimensions() is 1536

Scenario Outline: Provider dispatch resolves to the correct client type
  Given ai.embedding.provider is "<provider>"
    And a valid API key is configured for it
  When newDirectEmbeddingClientFromConfig is called
  Then no error is returned

  Examples:
    | provider |
    | local    |
    | openai   |
    | cohere   |
    | voyage   |
    | google   |

Scenario: Unknown provider is a clear, immediate error
  Given ai.embedding.provider is "not-a-real-provider"
  When newDirectEmbeddingClientFromConfig is called
  Then it returns an error mentioning "unknown provider"
    And no partial client is returned
```
(Covered by `TestNewEmbeddingClientFromConfig_*` in `internal/ai/ai_test.go` /
`ai_embedding_test.go`, and per-provider `TestNew*Client_*` tests.)

### Feature: openai-compatible requires an explicit endpoint
Ref: UC-02 (alternative flow)

```gherkin
Scenario: openai-compatible with no base URL is rejected
  Given ai.embedding.provider is "openai-compatible"
    And ai.embedding.base_url is unset
  When newDirectEmbeddingClientFromConfig is called
  Then it returns an error mentioning "base_url"

Scenario: openai-compatible with a base URL and no API key succeeds
  Given ai.embedding.provider is "openai-compatible"
    And ai.embedding.base_url is "http://localhost:11434/v1"
    And ai.embedding.api_key is unset
  When newDirectEmbeddingClientFromConfig is called
  Then no error is returned
    And the client sends no Authorization header
```
(Covered by `TestNewOpenAIEmbeddingClient_OpenAICompatibleAllowsNoKey`,
`TestPromptEmbeddingProviderOpenAICompatibleRequiresABaseURL`, and
`TestPromptEmbeddingProviderOpenAICompatibleStoresBaseURLAndSkipsAPIKey`.)

### Feature: Vector width is never guessed
Ref: UC-04

```gherkin
Scenario: An unknown model with no override is a construction-time error
  Given ai.embedding.provider is "openai-compatible"
    And ai.embedding.model is "some-custom-model"
    And ai.embedding.dimensions is unset
  When the provider's embedding client is constructed
  Then it returns an error mentioning "set ai.embedding.dimensions explicitly"
    And no client is returned

Scenario: An explicit dimensions override resolves construction
  Given ai.embedding.provider is "openai-compatible"
    And ai.embedding.model is "some-custom-model"
    And ai.embedding.dimensions is "1024"
  When the provider's embedding client is constructed
  Then no error is returned
    And Dimensions() is 1024
```
(Covered by `TestNew*EmbeddingClient_UnknownModelDimensionsIsAnError` and
`TestNewOpenAIEmbeddingClient_DimensionsOverrideResolves` across the provider test files.)

### Feature: A vector of any valid width is stored, not silently dropped
Ref: "What 'before' looks like" — the pre-existing bug this task fixes

```gherkin
Scenario: A non-empty embedding of any length is written to the row
  Given an entity has been embedded, producing a vector of length N (N > 0)
  When buildEntityRow is called with that vector
  Then the row's embedding column is set to the vector, not nil

Scenario: A failed embed (empty vector) is stored as no embedding
  Given an entity's embed call produced an empty/nil vector
  When buildEntityRow is called
  Then the row's embedding column is nil
    And the entity remains searchable by keyword
```
(Regression coverage: existing `internal/ast` and `internal/wiki` test suites pass
unchanged post-fix, confirming the default 768-dim/local path is untouched; see
Technical Debt for the acknowledged gap — no test exercises a non-768 width against
a real Lance table end-to-end.)

### Feature: Rerank provider selection and response reordering
Ref: UC-05

```gherkin
Scenario: local rerank provider preserves existing download-gated behavior
  Given ai.rerank.provider is unset (or "local")
  When NewRerankerFromConfig is called
  Then it calls NewCrossEncoderReranker exactly as before this task

Scenario Outline: A remote rerank provider requires an API key
  Given ai.rerank.provider is "<provider>"
    And no API key is configured or exported for it
  When NewRerankerFromConfig is called
  Then it returns an error before making any network call

  Examples:
    | provider |
    | cohere   |
    | voyage   |
    | jina     |

Scenario: A reordered provider response is mapped back to input order
  Given a rerank call sends 3 candidates in order [A, B, C]
    And the provider responds with results ordered [C, A, B] carrying each one's original index
  When Score is called
  Then the returned []float64 has C's score at position 2, A's at position 0, B's at position 1
```
(Covered by `TestNewRerankerFromConfig_*` and each provider's
`Test*Reranker_Score_MapsResultsBackToOriginalOrder` in `internal/ai`.)

## System Knowledge (additional, discovered mid-implementation)

- **Reranking has no production call site at all, today, independent of this task.** Grepped
  `cmd/`, `internal/mcpstdio`, `internal/uiserver`, `internal/daemon` for `CrossEncoder` /
  `RerankAdapter` / `search.rerank` / `Rerank:` — every hit outside `internal/ai` and
  `internal/lancestore` is either the config flag definition or a test. `search.rerank`,
  `ai.CrossEncoderReranker`, and `lancestore.RerankConfig.Reranker` all exist and work, but
  nothing in production ever constructs a reranker and passes it into a search call. T7's
  `ai.NewRerankerFromConfig` makes provider selection available and correct for whenever that
  end-to-end wiring gets built — it does not itself wire reranking into search, because doing
  so was never part of what was asked and is a separate, pre-existing gap.
- **The vector width bug this task fixes was worse than "unsupported providers".** Before this
  session, `buildEntityRow` (AST) and `buildChunkRow` (wiki) both gated on
  `len(emb) == ai.EmbeddingDimensions` (768) — so ANY vector of a different width was silently
  stored as if the entity/chunk had no embedding at all, no error, no log. Fixed to
  `len(emb) > 0`, which is the actually-correct distinguishing test (a failed embed call
  reports empty/nil; a valid one from any provider does not).
- **The embedding cache (`internal/ast/shard_emb_cache.go`) is self-describing** — its binary
  format already stores a `dim` field per shard — but the reader rejected anything whose stored
  dim did not equal the hardcoded constant, rather than trusting its own header. Fixed to
  compare against `ai.ResolveConfiguredEmbeddingDimensions()` (the ACTIVE provider's width), so
  a provider switch now correctly invalidates every existing shard through the pre-existing
  "unreadable or older format: drop it, it's a cache" path — no new invalidation logic needed.
- **Lance vector columns are fixed-width and strict** (confirmed via the existing "Dim must be
  set" / "fixed-width in Lance" comments in `internal/lancestore/lancestore.go`). This is what
  makes the T5 approach safe without building custom on-disk-schema introspection: an existing
  table built under one provider's width, written to with a different width after a provider
  switch, fails the Append/Upsert with a Lance/Arrow type error — a clear, fail-fast signal to
  reindex, not silent corruption. Deliberately not building a "detect and auto-migrate" path;
  the user confirmed a full reindex on provider switch is acceptable.
- `internal/ast/query.go`'s and `internal/wiki/embedder.go`'s pad/truncate-to-768 safety nets
  were replaced with a `fitVectorWidth(vec, dim)` helper keyed off the ACTUAL client's
  `Dimensions()` rather than the constant — kept as a safety net (a provider returning a
  slightly wrong-length vector no longer corrupts the query), not removed outright.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ai/embedding.go` | Modified | `Dimensions()` on the interface; config-driven factory (`newDirectEmbeddingClientFromConfig`), provider dispatch, `resolveAPIKey`/`firstNonEmpty` helpers |
| `internal/ai/embedding_dims.go` | Created | Known-model dimension table, provider defaults, `ResolveEmbeddingDimensions`, `ResolveConfiguredEmbeddingDimensions` |
| `internal/ai/embedding_lazy.go` | Modified | `LazyEmbeddingClient` now resolves the configured provider (via `newDirectEmbeddingClientFromConfig`), not hardcoded local — this is what makes the daemon provider-aware |
| `internal/ai/embedding_local.go` | Modified | Added `Dimensions() int` (returns 768) |
| `internal/ai/embedding_proxy.go` | Modified | Added `Dimensions()`, resolved from config rather than a socket round trip |
| `internal/ai/embedding_openai.go` | Created (agent) | OpenAI + openai-compatible client (shared wire format) |
| `internal/ai/embedding_cohere.go` | Created (agent) | Cohere embedding client |
| `internal/ai/embedding_voyage.go` | Created (agent) | Voyage AI embedding client |
| `internal/ai/embedding_google.go` | Created (agent) | Google Generative Language API embedding client |
| `internal/ai/embedding_remote_http.go` | Created (agent) | Shared HTTP POST/auth/error-handling helper for all remote clients |
| `internal/ai/rerank_dims.go` | Created (agent) | Default rerank model per provider |
| `internal/ai/rerank_cohere.go` / `rerank_voyage.go` / `rerank_jina.go` | Created (agent) | Remote rerank `Scorer` implementations |
| `internal/ai/rerank_config.go` | Created (agent) | `NewRerankerFromConfig` factory |
| `internal/ai/*_test.go` (10 new files, agent) | Created | Unit tests for every new client and the factory |
| `internal/ai/ai_test.go`, `ai_embedding_test.go` | Modified | Updated two assertions that hardcoded the local model's name in the lazy client's pre-init `ModelName()` string, now provider-generic |
| `internal/ast/search_lance.go` | Modified | `lanceEntitiesSchema(vectorDim int)` parameterized; write-gate fixed from `== ai.EmbeddingDimensions` to `> 0` |
| `internal/ast/query.go` | Modified | Query-vector pad/truncate now keyed off the active client's `Dimensions()`, not the constant; added `fitVectorWidth` |
| `internal/ast/embedder.go` | Modified | Same pad/truncate fix on the write path |
| `internal/ast/shard_emb_cache.go` | Modified | Binary cache reader/writer use the active resolved dimension instead of the hardcoded constant |
| `internal/ast/embed_batching_test.go`, `embedder_streaming_test.go` | Modified | Added `Dimensions()` to two test mock `EmbeddingClient` implementations |
| `internal/wiki/store.go` | Modified | Same schema-parameterization and write-gate fix as `search_lance.go`, for the wiki's chunk table |
| `internal/wiki/embedder.go` | Modified | Same pad/truncate fix as `internal/ast/query.go`; added `fitVectorWidth` |
| `internal/daemon/embedserver_test.go` | Modified | Added `Dimensions()` to the mock `EmbeddingClient` |
| `cmd/graphit/commands/setup.go` | Modified | `promptEmbeddingProvider`/`promptRerankProvider`, extracted `promptSecret`, made the local-model download conditional on `ai.embedding.provider == "local"` |
| `cmd/graphit/commands/setup_ai_provider_test.go` | Created | Tests for the two new setup prompts |
| `docs/specs/ai_engine.md` | Modified | Rewrote "Embedding Backends", added "Rerank Backends", updated the summary and the model-download section |
| `docs/tasks/embedding-and-rerank-provider-support.md` | Created | This task log |

## Trade-offs & Decisions

- **No generic rerank protocol.** Considered a "Cohere-compatible" generic rerank protocol
  (several self-hosted rerankers mimic Cohere's shape) but the user explicitly did not select
  it when offered — scope stays to three named providers plus local.
- **Provider switch requires a full reindex**, confirmed acceptable by the user. No
  multi-column / multi-dimension-simultaneously design was attempted — that was offered as
  an alternative and declined in favor of the simpler fail-fast-then-reindex approach.
- **LadybugDB is out of scope** — confirmed by the user it holds no embeddings, only the AST
  graph. All vector work is confined to `internal/lancestore`-backed stores.

## Technical Debt

- [ ] Reranking has no production call site (pre-existing, not introduced by this task —
  see System Knowledge above). `ai.NewRerankerFromConfig` is ready to be wired into
  `lancestore.RerankConfig.Reranker` whenever that end-to-end integration is built; it needs
  a small adapter satisfying `lancestore.Reranker` (`Rerank([]lancestore.Hit) ([]lancestore.Hit, error)`,
  `Name() string`) around `*ai.RerankAdapter` (`Rank([]ai.RerankHit) ([]ai.RerankHit, error)`).
- [ ] Only OpenAI's `text-embedding-3-small`/`-large` and `ada-002`, Cohere's four
  `embed-*-v3.0` variants, Voyage's four `voyage-*` models, and Google's two models are in
  the known-dimension table (`internal/ai/embedding_dims.go`). A newer model from any of
  these providers needs either a table entry added or `ai.embedding.dimensions` set
  explicitly — it fails clearly (construction-time error) rather than silently, but it does
  need one or the other.
- [ ] No automated test proves the AST/wiki vector-store dimension threading end-to-end
  against a non-768 provider (e.g. building a real Lance table at 1024-dim and confirming a
  768-dim write fails, or that a shard cache written at 1024-dim is dropped when the active
  provider resolves to 768). The logic was verified by code inspection plus the full
  existing test suite passing unchanged for the (default, 768-dim, local) path — a
  regression here would only surface when someone actually runs a non-local provider
  end-to-end.

## Verification

- `go build ./...` and `go build -tags lancedb ./...` — clean.
- `go vet -tags lancedb ./internal/ai/... ./internal/wiki/... ./internal/daemon/... ./cmd/graphit/commands/...` — clean.
- `gofmt -l` on every touched package — clean (the one flagged file, `cmd/graphit/commands/runners.go`,
  was already unformatted before this task and was not touched).
- `go test -tags lancedb ./internal/ai/... ./internal/ast/... ./internal/wiki/... ./internal/daemon/... ./cmd/graphit/commands/...` —
  all pass, including ~150 new/updated tests across the new provider clients, the rerank
  factory, and the setup prompts.

## Progress Log

### 2026-08-29
- Completed T0: research via graphit MCP tools (memory, AST graph, knowledge wiki) and
  scope confirmation with the user via `AskUserQuestion`. Starting T1.
- Completed T1/T2/T4/T5/T8 directly: `EmbeddingClient.Dimensions()`, the config-driven
  factory (`internal/ai/embedding.go`), the dimension resolution helpers
  (`internal/ai/embedding_dims.go`), the daemon fix (`LazyEmbeddingClient` now resolves the
  configured provider instead of hardcoding local — no change needed to `daemon.go` itself,
  it already called `NewLazyEmbeddingClient`), the AST search index and wiki store schema/
  write-gate/embedding-cache dimension threading (`internal/ast/search_lance.go`,
  `query.go`, `embedder.go`, `shard_emb_cache.go`, `internal/wiki/store.go`,
  `embedder.go`), and the `graphit setup` prompts (`cmd/graphit/commands/setup.go`,
  extracting a reusable `promptSecret` helper). Discovered and fixed a real pre-existing bug
  along the way: the vector write-gate compared `len(emb) == ai.EmbeddingDimensions`, which
  silently dropped every valid vector from any would-be non-768 provider — fixed to
  `len(emb) > 0`.
- Delegated T3 (remote embedding clients: OpenAI/openai-compatible, Cohere, Voyage, Google)
  and T6/T7 (remote rerank clients: Cohere, Voyage, Jina, plus the `NewRerankerFromConfig`
  factory) to a background agent, with the exact struct/function contracts already fixed in
  `embedding.go` given as its interface to build against. It delivered 9 implementation
  files + 10 test files under `internal/ai/`, gofmt-clean, fully passing. Reviewed its two
  key files (`embedding_openai.go`, `rerank_config.go`) directly — matches the contract
  exactly. Two minor, reasonable deviations from the original spec noted by the agent itself
  (OpenAI dimensions-override detection reads the already-resolved dim instead of
  re-reading config; Voyage/Jina rerank send one unchunked request since neither spec nor
  their real APIs stated a limit) — both accepted as-is.
- Completed T9: rewrote `docs/specs/ai_engine.md`'s Embedding Backends section (it
  previously stated flatly "There is no external embedding provider" — a prior memory had
  already flagged this file as containing stale claims) and added a new Rerank Backends
  section, including the "no production call site yet" note as an explicit, honest scope
  boundary rather than leaving a reader to assume reranking is fully wired end to end.
- Final verification (T10): full build (default + `lancedb` tags), `go vet`, `gofmt -l`,
  and the complete test suite for every touched package — all clean. Status: done.
- User asked "está tudo documentado?" (is everything documented?). Ran `graphit_knowledge_lint`
  to check rather than assume — the project-wide report (244 pages, 129 pre-existing errors,
  unrelated to this task) surfaced one real mistake from this session: a memory I wrote
  referenced a truncated “Embedding is 100% local…” memory title instead of the real
  one, made worse by having renamed that memory's title in the same turn (which changes its
  compiled slug). Fixed both the memory's wikilink and this doc's equivalent reference.
  Also brought this log up to the Full Task Log Template's mandatory bar for a feature this
  size — it was missing `## Use Cases`, `## Test Cases & Acceptance Criteria` (Gherkin), and
  `## Files Changed`, and had two duplicate `## System Knowledge` headings from separate
  edit passes; all fixed. Deliberately did NOT create formal OpenAPI spec files for the 5
  newly-consumed external REST APIs (OpenAI, Cohere, Voyage, Google, Jina): checked first —
  zero precedent exists anywhere in this project's 244-page wiki for documenting a *consumed*
  third-party API that way (HuggingFace's model download and the S3/Hub integration are both
  documented as prose in `docs/specs/*.md`, not as OpenAPI YAML) — matching the established
  convention already in `docs/specs/ai_engine.md`'s tables was the right call over inventing
  a format never used here.
