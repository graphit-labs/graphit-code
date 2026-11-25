use crate::error::{DataReadError, Result};
use graphit_core::{
    config::{ConfigKey, ConfigSnapshot},
    file_io::SnapshotContext,
};
use std::io;

/// Bounds on graph metadata inputs and individual query decoding.
#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(default = "default_limits", deny_unknown_fields)]
pub struct LocalGraphLimits {
    pub manifest_bytes: usize,
    pub max_tables: usize,
    pub max_members: usize,
    pub max_columns: usize,
    pub max_name_bytes: usize,
    /// Rows decoded by one sample or neighborhood operation, never corpus admission.
    pub query_decoded_rows: usize,
    pub max_row_groups: usize,
    pub max_footer_bytes: usize,
    pub max_protocol_bytes: usize,
    pub max_list_entries: usize,
    pub max_protocol_entries: usize,
    pub max_header_bytes: usize,
    pub max_metadata_entries: usize,
    pub max_metadata_bytes: usize,
    pub max_chunk_bytes: usize,
    pub max_pages: usize,
    pub max_page_compressed_bytes: usize,
    pub max_page_uncompressed_bytes: usize,
}

// Application input bounds, not allocations or quotas on live requests.
fn default_limits() -> LocalGraphLimits {
    LocalGraphLimits {
        manifest_bytes: 4194304,
        max_tables: 1024,
        max_members: 4096,
        max_columns: 1024,
        max_name_bytes: 4096,
        query_decoded_rows: 1000000000,
        max_row_groups: 65536,
        max_footer_bytes: 16777216,
        max_protocol_bytes: 16777216,
        max_list_entries: 1048576,
        max_protocol_entries: 4194304,
        max_header_bytes: 65536,
        max_metadata_entries: 65536,
        max_metadata_bytes: 16777216,
        max_chunk_bytes: 1073741824,
        max_pages: 1048576,
        max_page_compressed_bytes: 33554432,
        max_page_uncompressed_bytes: 67108864,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LocalGraphPolicy(pub(crate) LocalGraphLimits);
impl LocalGraphPolicy {
    /// Captured configuration key for an optional object of input limits.
    pub const CONFIG_KEY: &'static str = "data.local_graph";

    /// Resolve decoder input bounds from the caller's effective configuration.
    ///
    /// An absent setting uses the documented application defaults; a partial
    /// object overrides those defaults. The highest configuration layer selects
    /// the entire object. This does not read files/environment or grant access.
    ///
    /// ```
    /// use graphit_core::{brand::Brand, config::ConfigSnapshot, file_io::SnapshotContext,
    ///     file_lock::Cancellation};
    /// use graphit_data::LocalGraphPolicy;
    /// use std::time::{Duration, Instant};
    /// let config = ConfigSnapshot::new(&Brand::default(), []).unwrap();
    /// let original = SnapshotContext::new(Instant::now() + Duration::from_secs(10), Cancellation::default());
    /// let policy = LocalGraphPolicy::resolve(&config, &original).unwrap();
    /// assert!(policy.limits().manifest_bytes > 0);
    /// ```
    pub fn resolve(config: &ConfigSnapshot, original: &SnapshotContext) -> io::Result<Self> {
        original.check()?;
        let key = ConfigKey::new(Self::CONFIG_KEY)?;
        let limits = match config.get::<serde_json::Map<String, serde_json::Value>>(&key)? {
            None => default_limits(),
            Some(object) => serde_json::from_value(serde_json::Value::Object(object))
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
        };
        let policy =
            Self::new(limits).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        original.check()?;
        Ok(policy)
    }

    pub fn new(limits: LocalGraphLimits) -> Result<Self> {
        if limits.manifest_bytes == 0 {
            return Err(DataReadError::InvalidPolicy("manifest_bytes"));
        }
        if limits.max_tables == 0 {
            return Err(DataReadError::InvalidPolicy("max_tables"));
        }
        if limits.max_members == 0 {
            return Err(DataReadError::InvalidPolicy("max_members"));
        }
        if limits.max_columns == 0 {
            return Err(DataReadError::InvalidPolicy("max_columns"));
        }
        if limits.max_name_bytes == 0 {
            return Err(DataReadError::InvalidPolicy("max_name_bytes"));
        }
        if limits.query_decoded_rows == 0 {
            return Err(DataReadError::InvalidPolicy("query_decoded_rows"));
        }
        if limits.max_row_groups == 0 {
            return Err(DataReadError::InvalidPolicy("max_row_groups"));
        }
        if limits.max_footer_bytes == 0 {
            return Err(DataReadError::InvalidPolicy("max_footer_bytes"));
        }
        if limits.max_protocol_bytes == 0 {
            return Err(DataReadError::InvalidPolicy("max_protocol_bytes"));
        }
        if limits.max_list_entries == 0 {
            return Err(DataReadError::InvalidPolicy("max_list_entries"));
        }
        if limits.max_protocol_entries == 0 {
            return Err(DataReadError::InvalidPolicy("max_protocol_entries"));
        }
        if limits.max_header_bytes == 0 {
            return Err(DataReadError::InvalidPolicy("max_header_bytes"));
        }
        if limits.max_metadata_entries == 0 {
            return Err(DataReadError::InvalidPolicy("max_metadata_entries"));
        }
        if limits.max_metadata_bytes == 0 {
            return Err(DataReadError::InvalidPolicy("max_metadata_bytes"));
        }
        if limits.max_chunk_bytes == 0 {
            return Err(DataReadError::InvalidPolicy("max_chunk_bytes"));
        }
        if limits.max_pages == 0 {
            return Err(DataReadError::InvalidPolicy("max_pages"));
        }
        if limits.max_page_compressed_bytes == 0 {
            return Err(DataReadError::InvalidPolicy("max_page_compressed_bytes"));
        }
        if limits.max_page_uncompressed_bytes == 0 {
            return Err(DataReadError::InvalidPolicy("max_page_uncompressed_bytes"));
        }

        if limits.max_tables > u32::MAX as usize || limits.max_members > u32::MAX as usize {
            return Err(DataReadError::InvalidPolicy("identity count"));
        }
        Ok(Self(limits))
    }
    pub fn limits(&self) -> LocalGraphLimits {
        self.0
    }
}
/// Explicit sampling and decoded-work limits. A 300/1000 sample needs 2300 nodes.
#[derive(Clone, Copy, Debug)]
pub struct GraphSampleLimits {
    pub(crate) seed_nodes: usize,
    pub(crate) global_edges: usize,
    pub(crate) max_nodes: usize,
    pub(crate) max_string_bytes: usize,
    pub(crate) max_decoded_rows: usize,
    pub(crate) max_page_work: usize,
    pub(crate) row_batch_size: usize,
}
impl GraphSampleLimits {
    pub fn new(
        seed_nodes: usize,
        global_edges: usize,
        max_nodes: usize,
        max_string_bytes: usize,
        max_decoded_rows: usize,
        max_page_work: usize,
        row_batch_size: usize,
    ) -> Result<Self> {
        if [
            seed_nodes,
            global_edges,
            max_nodes,
            max_string_bytes,
            max_decoded_rows,
            max_page_work,
            row_batch_size,
        ]
        .contains(&0)
        {
            return Err(DataReadError::InvalidPolicy("sample limits"));
        }
        let nodes = global_edges
            .checked_mul(2)
            .and_then(|n| n.checked_add(seed_nodes))
            .ok_or(DataReadError::InvalidPolicy("max_nodes"))?;
        if row_batch_size > max_decoded_rows {
            return Err(DataReadError::InvalidPolicy("row_batch_size"));
        }
        if max_nodes < nodes {
            return Err(DataReadError::InvalidPolicy("max_nodes"));
        }
        Ok(Self {
            seed_nodes,
            global_edges,
            max_nodes,
            max_string_bytes,
            max_decoded_rows,
            max_page_work,
            row_batch_size,
        })
    }
}

/// Decoder work shared by samples and neighborhoods; no synthetic sampling policy.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DecodeLimits {
    pub(crate) max_decoded_rows: usize,
    pub(crate) max_page_work: usize,
    pub(crate) row_batch_size: usize,
}
impl From<GraphSampleLimits> for DecodeLimits {
    fn from(value: GraphSampleLimits) -> Self {
        Self {
            max_decoded_rows: value.max_decoded_rows,
            max_page_work: value.max_page_work,
            row_batch_size: value.row_batch_size,
        }
    }
}
