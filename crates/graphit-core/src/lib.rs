#![deny(unsafe_code)]
#![forbid(unsafe_op_in_unsafe_fn)]
//! Shared, native-library-free contracts for Graphit's Rust implementation.

pub mod artifact_store;
pub mod auth_document;
pub mod brand;
pub mod config;
pub mod file_io;
pub mod file_lock;
pub mod git_checkout;
pub mod git_hooks;
pub mod git_record_event;
pub mod ignore_rules;
pub mod installation_bootstrap;
pub mod installation_identity;
pub mod local_corpus;
pub mod local_files;
pub mod log_file;
pub mod log_tail;
pub mod module_settings;
pub mod native_document_file;
pub mod native_document_lock;
pub mod native_file_identity;
pub mod native_path;
pub mod native_principal;
pub mod pagination;
mod private_document;
mod process;
pub mod project_identity;
pub mod project_inventory;
pub mod project_paths;
pub mod project_root;
pub mod project_version;
pub mod runtime_key;
pub mod startup_priority;
pub mod structured_output;
pub mod tcp_listener;
pub mod terminal;
pub mod text_slice;
pub mod version_context;
