//! Explicit configuration layers and protected inspection, independent of adapters.
mod document;
mod key;
mod snapshot;
mod store;
mod value;

pub use document::ConfigDocument;
pub(crate) use document::{FILE_LIMIT, RawSettings, count_serialized, present_value};
pub use key::{ConfigKey, SECRET_KEYS};
pub use snapshot::ConfigSnapshot;
pub use store::{ConfigMutation, ConfigStore, ConfigWriter};
pub use value::{ConfigInspection, ConfigSource, InspectionValue};
