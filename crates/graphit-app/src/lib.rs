//! Public App producers; shared CLI routing belongs to the application owner.

// Reclaim freed pages after background jobs, including native malloc/new users.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[global_allocator]
static ALLOCATOR: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

mod acp_process;
pub mod ast_export_entry;
mod ast_parser_resources;
mod ast_profile_loader;
pub mod ast_semantic;
pub mod ast_source;
pub mod auth_cli;
pub mod auth_entry;
pub mod browser_auth;
pub mod cli_cursor;
mod cli_launch_policy;
pub mod compiled_skills;
pub mod config_entry;
pub mod daemon_control;
pub mod daemon_launcher;
mod embedding_metadata;
pub mod embedding_service;
mod foreground_ast_sources;
mod foreground_dream;
mod foreground_knowledge;
mod foreground_knowledge_sources;
mod foreground_memory;
pub mod foreground_memory_jobs;
mod foreground_now;
pub mod foreground_project_control;
mod foreground_record_references;
pub mod foreground_source;
mod foreground_task_embeddings;
mod foreground_task_references;
mod foreground_tasks;
mod foreground_ui;
mod foreground_user_memory;
pub mod git_hook_commands;
pub mod git_records;
pub mod graph_entry;
pub mod graph_server;
mod instruction_overrides;
pub mod knowledge_ask;
pub mod knowledge_ask_entry;
mod knowledge_ask_service;
pub mod knowledge_export;
pub mod knowledge_export_entry;
mod knowledge_kiro_process;
pub mod knowledge_live;
mod knowledge_log;
pub mod knowledge_log_entry;
pub mod knowledge_package_export;
pub mod knowledge_question;
pub mod knowledge_retirement;
pub mod knowledge_search_entry;
pub mod knowledge_semantic;
pub mod knowledge_source;
pub mod knowledge_source_entry;
mod live_agent_ast;
mod live_agent_catalog;
mod live_agent_contexts;
mod live_agent_preparation;
mod live_agent_read;
mod live_agent_tools;
mod live_project_selections;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub mod local_file_install;
pub mod managed_role_run;
pub mod managed_roles;
pub(crate) mod mcp_output;
pub mod mcp_source;
pub mod memory_index_entry;
pub mod model_provision;
pub mod models;
pub mod project_coordinator_install;
pub(crate) mod project_graph;
pub mod project_hook_context;
pub mod project_hook_install;
pub mod project_retirement;
mod reference_explorer;
mod search_rerank;
pub mod self_update;
pub mod self_update_release;
pub mod service_entry;
pub mod session_hook_entry;
pub mod source_entry;
pub mod source_server;
pub mod sync_entry;
pub mod task_entry;
pub mod task_server;
#[cfg(target_os = "linux")]
pub mod tray_entry;
pub mod ui_entry;

pub mod admin_auth;
pub mod auth_active;

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub(crate) mod authored_resources;
pub(crate) mod file_publication;
mod hub_project_metadata;
pub mod hub_publish;
pub(crate) mod hub_unpublish;
pub mod native_hub_install;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub(crate) mod native_publication;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub(crate) mod native_storage_credentials;
pub(crate) mod received_native;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub(crate) mod record_storage;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub mod remote_file_install;
pub mod remote_hub_catalog;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub(crate) mod remote_native_mount;

pub mod native_install;
pub mod native_install_client;
pub mod native_install_entry;
pub mod native_install_http;

pub mod now_entry;
pub mod now_renderer;
pub mod now_transport;

pub mod role_proxy;
pub mod role_tool_server;

pub mod knowledge_conversation;

#[cfg(test)]
#[path = "../tests/support/agent_fixture.rs"]
pub(crate) mod test_agent_fixture;
#[cfg(all(test, target_os = "linux"))]
#[path = "../tests/support/dream_peer.rs"]
pub(crate) mod test_dream_peer;

#[cfg(test)]
#[path = "../../graphit-ai/tests/support/rerank_http.rs"]
mod rerank_test_http;
