#![recursion_limit = "256"]
//! App integration journeys share one libtest executable and retain their modules.

#[cfg(target_os = "linux")]
use graphit_app::{
    native_install_http::NativeInstallPolicy,
    source_server::{SourcePolicy, SourceServer},
};
use graphit_daemon::model_service as model_api;
#[cfg(target_os = "linux")]
use knowledge_ask_server as fixture_support;
#[cfg(target_os = "linux")]
use knowledge_ask_server as support;

#[cfg(target_os = "linux")]
#[path = "support/agent_fixture.rs"]
mod agent_fixture;
#[path = "support/broker_oauth.rs"]
mod broker_oauth;
#[path = "support/finite_cli.rs"]
mod finite_cli;
#[cfg(target_os = "linux")]
#[path = "../../graphit-ai/tests/support/kimi_peer.rs"]
mod kimi_peer;
#[cfg(target_os = "linux")]
#[path = "support/knowledge_ask_server.rs"]
mod knowledge_ask_server;
#[cfg(target_os = "linux")]
#[path = "support/knowledge_cli_daemon.rs"]
mod knowledge_cli_daemon;
#[allow(
    dead_code,
    reason = "shared model fixture includes RPC and native qualification helpers used by other tests"
)]
#[path = "../../graphit-daemon/tests/support/model_fixture.rs"]
mod model_fixture;
#[path = "support/native_install_auth.rs"]
mod native_install_auth;
#[cfg(target_os = "linux")]
#[path = "support/now_fixture.rs"]
mod now_fixture;
#[allow(
    dead_code,
    reason = "shared wire fixture includes response barriers exercised by the AI provider suites"
)]
#[path = "../../graphit-ai/tests/support/rerank_http.rs"]
mod rerank_http;
#[cfg(unix)]
#[path = "support/runtime_peer.rs"]
mod runtime_peer;
#[path = "support/source_client.rs"]
mod source_client;

mod ast_generic_nn_executable;
mod ast_semantic;
mod ast_source;
mod auth_active;
mod auth_entry;
mod auth_executable;
mod browser_auth_start;
mod config_executable;
mod daemon_autostart;
mod daemon_control;
mod daemon_launcher;
mod daemon_service_entry;
mod embedding_service;
mod foreground_ast_semantic;
mod foreground_broker_inventory;
#[cfg(feature = "postgres-qualification")]
mod foreground_now;
mod foreground_source;
mod foreground_tasks;
mod git_hooks_executable;
mod help_cli;
mod knowledge_antigravity_ask;
mod knowledge_ask_ingress;
mod knowledge_ask_stdio;
mod knowledge_claude_ask;
mod knowledge_conversation;
mod knowledge_copilot_ask;
mod knowledge_cursor_ask;
mod knowledge_export;
mod knowledge_grok_ask;
mod knowledge_kimi_ask;
mod knowledge_opencode_ask;
mod knowledge_question;
mod knowledge_qwen_ask;
mod knowledge_retirement;
mod knowledge_semantic;
mod knowledge_source;
mod knowledge_source_cli;
mod managed_role_run;
mod managed_roles;
mod mcp_source;
mod model_provision;
mod models_cli;
mod native_install;
mod native_install_executable;
mod native_install_http;
mod now_executable;
mod now_presence_integration;
mod now_tools;
mod now_transport;
mod package;
mod project_coordinator_install;
mod project_hook_context;
mod project_hook_install;
mod project_retirement;
mod role_hook_v3_executable;
mod source_cli;
mod source_executable;
mod source_server;
mod task_executable;
mod version_cli;
