---
title: AI integration tests
description: Run the AI contracts through one libtest target while preserving module filters, native peers and platform boundaries.
---

# AI integration tests

After preparing the locked dependencies and native toolchain described in
[Rust workspace checkout](rust-workspace.md), run the external AI contracts with:

```sh
cargo xtask cargo test --locked --offline -p graphit-ai --test integration
cargo xtask cargo clippy --locked --offline -p graphit-ai --test integration --no-deps -- -D warnings
```

The [AI manifest](../../crates/graphit-ai/Cargo.toml) declares one external test
target. Its [composition](../../crates/graphit-ai/tests/integration.rs) loads the
existing suites as ordinary Rust modules and imports each shared support module
once. Cargo links one integration executable; libtest retains its normal parallel
execution. Every case still owns its temporary roots, loopback listeners and
Process hosts. No suite-wide lock or serialized runner is introduced. Reuse the
canonical build directory described in [Rust checks](rust-fast-guard.md).

## Select a boundary

Test names include their module prefix. A Cargo filter selects that prefix:

```sh
cargo xtask cargo test --locked --offline -p graphit-ai --test integration direct_native_rerank::
cargo xtask cargo test --locked --offline -p graphit-ai --test integration knowledge_conversation::opencode::
cargo xtask cargo test --locked --offline -p graphit-ai --test integration -- --list
```

| Boundary | Module prefixes |
|---|---|
| Native completion processes | `cli_antigravity::`, `cli_claude::`, `cli_codex::`, `cli_copilot::`, `cli_cursor::`, `cli_grok::`, `cli_kimi::`, `cli_kimi_print::`, `cli_kiro::`, `cli_opencode::`, `cli_plain::`, `cli_qwen::` |
| Embedding SDKs and Auth leases | `cohere_embeddings::`, `voyage_embeddings::`, `direct_embeddings::`, `google_embeddings::`, `direct_embedding_leases::` |
| Reranking, wire and validation | `cohere_reranking::`, `voyage_reranking::`, `jina_reranking::`, `graphit_reranking::`, `direct_native_rerank::`, `native_rerank::`, `ranking::` |
| Private conversation records | `knowledge_conversation::`, including its provider submodules |

With `--exact`, supply the complete module and function name. The
[Kimi process suite](../../crates/graphit-ai/tests/cli_kimi.rs) launches the same
libtest executable with `--ignored --exact cli_kimi::native_kimi_peer`. This
ignored entry point belongs to the child protocol and is invoked by its parent
tests. The separate `ai-cli-plain-fixture` and `ai-copilot-fixture` binaries retain
their Cargo-provided executable paths.

## Coverage and platform limits

The consolidation preserves all 25 suites and their existing cases. The Linux
inventory contains 315 cases: 308 ordinary tests and seven ignored entries
(six optional measurements and the Kimi child entry point). Optional measurements
remain separate, explicitly selected qualifications. Historical receipts may name
the earlier individual Cargo targets; current commands use `integration` and the
module filters above.

The conversation-record module keeps its existing Linux condition, and the
Unix-only Kiro case keeps its condition. A Linux run does not qualify Windows or
macOS. Controlled peers use real process, TCP, Auth and private-file boundaries;
these tests require no vendor account, installed vendor CLI or downloaded model.
They do not prove an authenticated vendor call or model inference. Native model
unit tests remain with the library and are outside this external target.

Related contracts: [Direct embedding leases](../specs/rust_direct_embedding_leases.md),
[Direct native reranking](../specs/rust_direct_native_rerank.md),
[conversation records](../specs/rust_knowledge_conversation.md) and
[native Process ownership](../specs/rust_process_incremental.md).
