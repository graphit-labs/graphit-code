//! External AI contracts share one libtest executable and retain their own fixtures.

#[path = "support/cli_control.rs"]
mod cli_control;
#[path = "support/direct_rerank_auth.rs"]
mod direct_rerank_auth;
#[path = "support/kimi_cleanup.rs"]
mod kimi_cleanup;
#[path = "support/rerank_http.rs"]
mod rerank_http;

mod cli_antigravity;
mod cli_claude;
mod cli_codex;
mod cli_copilot;
mod cli_cursor;
mod cli_grok;
mod cli_kimi;
mod cli_kimi_print;
mod cli_kiro;
mod cli_opencode;
mod cli_plain;
mod cli_qwen;
mod cohere_embeddings;
mod cohere_reranking;
mod direct_embedding_leases;
mod direct_embeddings;
mod direct_native_rerank;
mod google_embeddings;
mod graphit_reranking;
mod jina_reranking;
mod knowledge_conversation;
mod native_rerank;
mod ranking;
mod voyage_embeddings;
mod voyage_reranking;
