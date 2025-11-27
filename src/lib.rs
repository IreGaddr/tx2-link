pub mod protocol;
pub mod transport;
pub mod serialization;
pub mod compression;
pub mod rate_limit;
pub mod schema;
pub mod error;
pub mod sync;

pub use protocol::{
    EntityId, ComponentId, FieldId,
    Message, MessageType, MessageHeader,
    DeltaChange, FieldDelta,
};

pub use serialization::{
    SerializedComponent, SerializedEntity, WorldSnapshot, Delta,
    BinarySerializer, BinaryFormat,
};

pub use transport::{
    Transport, TransportError,
};

pub use compression::{
    DeltaCompressor, FieldCompressor,
};

pub use rate_limit::{
    RateLimiter, RateLimitConfig,
};

pub use schema::{
    ComponentSchema, FieldSchema, SchemaRegistry, SchemaVersion,
};

pub use error::{
    LinkError, Result,
};

pub use sync::{
    SyncManager, SyncConfig, SyncMode,
};
