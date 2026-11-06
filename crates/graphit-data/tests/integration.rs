//! External Data contracts share one test binary; each module owns its fixtures.

#[path = "support/knowledge_manifest_fixture.rs"]
mod knowledge_manifest_fixture;
#[path = "support/knowledge_reference_fixture.rs"]
mod knowledge_reference_fixture;

mod ast_corpus_maintenance;
mod graph_search_sidecar;
mod knowledge_branch_materialization;
mod knowledge_document_projection;
mod knowledge_document_update;
mod knowledge_embedding_stream;
mod knowledge_embeddings;
mod knowledge_embeddings_sdk;
mod knowledge_first_publication;
mod knowledge_index_page;
mod knowledge_module_snapshot;
mod knowledge_reference_query;
mod knowledge_retirement;
mod knowledge_schema_admission;
mod knowledge_search;
mod knowledge_source_identity;
mod knowledge_source_stream;
mod lance_branch_store;
mod lance_code_fts_append;
mod lance_fts_index;
mod lance_fts_index_state;
mod lance_fts_index_store;
mod lance_git_refs;
mod lance_integrity;
mod lance_module_creation;
mod lance_module_store;
mod lance_native_branch_clone;
mod lance_native_clone;
mod lance_native_metadata;
mod lance_native_snapshot;
mod lance_nullable_vector;
mod lance_source_stream;
mod native_storage_uri;
mod vector_distance;
mod wiki_corpus;
