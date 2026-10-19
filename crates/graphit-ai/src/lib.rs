//! Inference consumers with explicit execution context and finite policies.
pub mod cli_plain;
pub mod cohere_embedding;
pub mod cohere_rerank;
pub mod context;
pub mod embedding;
pub mod gguf_rerank;
pub mod google_embedding;
pub mod graphit_embedding;
pub mod graphit_rerank;
pub mod model_artifact;
pub mod model_bindings;
pub mod model_manifest;
pub mod native_embedding;
pub mod native_rerank;
pub mod openai;
pub mod ranking;
pub mod voyage_embedding;

mod rerank_wire;

#[cfg(test)]
mod model_bindings_tests;

pub mod local_execution;

pub mod jina_rerank;

pub mod voyage_rerank;

pub mod direct_native_rerank;

pub mod direct_embedding;

pub mod cli_codex;
pub mod cli_codex_wire;
pub mod codex_completion;

pub mod cli_claude;
pub mod cli_claude_wire;
pub mod cli_cursor;
pub mod cli_cursor_wire;
pub mod cli_qwen;
pub mod cli_qwen_wire;

pub mod cli_grok;
pub mod cli_grok_wire;

pub mod cli_kiro;
pub mod cli_kiro_wire;

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub mod knowledge_conversation;

mod cli_acp_support;

pub mod cli_opencode;
pub mod cli_opencode_wire;

pub mod cli_kimi;
pub mod cli_kimi_print;

pub mod cli_copilot;

pub mod cli_provider;

pub mod cli_antigravity;
pub mod cli_antigravity_wire;

mod embedding_wire;

// Link the one Cargo-built native engine used by the CXX adapter.
use llama_cpp_sys_2 as _;

#[cfg(test)]
#[path = "../tests/support/rerank_http.rs"]
mod rerank_http;
