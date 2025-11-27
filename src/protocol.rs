use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type EntityId = u32;
pub type ComponentId = String;
pub type FieldId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum MessageType {
    Snapshot = 0,
    Delta = 1,
    RequestSnapshot = 2,
    Ack = 3,
    Ping = 4,
    Pong = 5,
    SchemaSync = 6,
    Error = 7,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageHeader {
    pub msg_type: MessageType,
    pub timestamp: u64,
    pub id: u64,
    pub sequence: u64,
    pub schema_version: u32,
}

impl MessageHeader {
    pub fn new(msg_type: MessageType, schema_version: u32) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};

        static mut SEQUENCE_COUNTER: u64 = 0;
        let sequence = unsafe {
            SEQUENCE_COUNTER += 1;
            SEQUENCE_COUNTER
        };

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let id = (timestamp << 20) | (sequence & 0xFFFFF);

        Self {
            msg_type,
            timestamp,
            id,
            sequence,
            schema_version,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub header: MessageHeader,
    pub payload: MessagePayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessagePayload {
    Snapshot(SnapshotPayload),
    Delta(DeltaPayload),
    RequestSnapshot,
    Ack { ack_id: u64 },
    Ping,
    Pong,
    SchemaSync(SchemaSyncPayload),
    Error { code: u32, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotPayload {
    pub entities: Vec<SerializedEntity>,
    pub metadata: SnapshotMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub world_time: f64,
    pub entity_count: u32,
    pub component_count: u32,
    pub compression: CompressionType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum CompressionType {
    None = 0,
    Deflate = 1,
    Lz4 = 2,
    Zstd = 3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedEntity {
    pub id: EntityId,
    pub components: Vec<SerializedComponent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedComponent {
    pub id: ComponentId,
    pub data: ComponentData,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComponentData {
    Binary(Vec<u8>),
    Json(String),
    Structured(HashMap<FieldId, FieldValue>),
}

impl ComponentData {
    pub fn from_json_value(value: serde_json::Value) -> Self {
        ComponentData::Json(value.to_string())
    }

    pub fn to_json_value(&self) -> Option<serde_json::Value> {
        match self {
            ComponentData::Json(s) => serde_json::from_str(s).ok(),
            _ => None,
        }
    }

    pub fn as_json_str(&self) -> Option<&str> {
        match self {
            ComponentData::Json(s) => Some(s),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldValue {
    Null,
    Bool(bool),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    String(String),
    Bytes(Vec<u8>),
    Array(Vec<FieldValue>),
    Map(HashMap<String, FieldValue>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaPayload {
    pub changes: Vec<DeltaChange>,
    pub base_timestamp: u64,
    pub metadata: DeltaMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaMetadata {
    pub change_count: u32,
    pub entities_added: u32,
    pub entities_removed: u32,
    pub components_updated: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeltaChange {
    EntityAdded {
        entity_id: EntityId,
    },
    EntityRemoved {
        entity_id: EntityId,
    },
    ComponentAdded {
        entity_id: EntityId,
        component_id: ComponentId,
        data: ComponentData,
    },
    ComponentRemoved {
        entity_id: EntityId,
        component_id: ComponentId,
    },
    ComponentUpdated {
        entity_id: EntityId,
        component_id: ComponentId,
        data: ComponentData,
    },
    FieldsUpdated {
        entity_id: EntityId,
        component_id: ComponentId,
        fields: Vec<FieldDelta>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDelta {
    pub field_id: FieldId,
    pub old_value: Option<FieldValue>,
    pub new_value: FieldValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaSyncPayload {
    pub schemas: Vec<ComponentSchemaInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentSchemaInfo {
    pub component_id: ComponentId,
    pub version: u32,
    pub fields: Vec<FieldSchemaInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSchemaInfo {
    pub field_id: FieldId,
    pub field_type: FieldType,
    pub optional: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum FieldType {
    Null = 0,
    Bool = 1,
    U8 = 2,
    U16 = 3,
    U32 = 4,
    U64 = 5,
    I8 = 6,
    I16 = 7,
    I32 = 8,
    I64 = 9,
    F32 = 10,
    F64 = 11,
    String = 12,
    Bytes = 13,
    Array = 14,
    Map = 15,
}

impl Message {
    pub fn new(msg_type: MessageType, schema_version: u32, payload: MessagePayload) -> Self {
        Self {
            header: MessageHeader::new(msg_type, schema_version),
            payload,
        }
    }

    pub fn snapshot(entities: Vec<SerializedEntity>, world_time: f64, schema_version: u32) -> Self {
        let entity_count = entities.len() as u32;
        let component_count: u32 = entities.iter()
            .map(|e| e.components.len() as u32)
            .sum();

        Self::new(
            MessageType::Snapshot,
            schema_version,
            MessagePayload::Snapshot(SnapshotPayload {
                entities,
                metadata: SnapshotMetadata {
                    world_time,
                    entity_count,
                    component_count,
                    compression: CompressionType::None,
                },
            }),
        )
    }

    pub fn delta(changes: Vec<DeltaChange>, base_timestamp: u64, schema_version: u32) -> Self {
        let change_count = changes.len() as u32;
        let entities_added = changes.iter()
            .filter(|c| matches!(c, DeltaChange::EntityAdded { .. }))
            .count() as u32;
        let entities_removed = changes.iter()
            .filter(|c| matches!(c, DeltaChange::EntityRemoved { .. }))
            .count() as u32;
        let components_updated = changes.iter()
            .filter(|c| matches!(c, DeltaChange::ComponentUpdated { .. } | DeltaChange::FieldsUpdated { .. }))
            .count() as u32;

        Self::new(
            MessageType::Delta,
            schema_version,
            MessagePayload::Delta(DeltaPayload {
                changes,
                base_timestamp,
                metadata: DeltaMetadata {
                    change_count,
                    entities_added,
                    entities_removed,
                    components_updated,
                },
            }),
        )
    }

    pub fn request_snapshot(schema_version: u32) -> Self {
        Self::new(
            MessageType::RequestSnapshot,
            schema_version,
            MessagePayload::RequestSnapshot,
        )
    }

    pub fn ack(ack_id: u64, schema_version: u32) -> Self {
        Self::new(
            MessageType::Ack,
            schema_version,
            MessagePayload::Ack { ack_id },
        )
    }

    pub fn ping(schema_version: u32) -> Self {
        Self::new(MessageType::Ping, schema_version, MessagePayload::Ping)
    }

    pub fn pong(schema_version: u32) -> Self {
        Self::new(MessageType::Pong, schema_version, MessagePayload::Pong)
    }

    pub fn error(code: u32, message: String, schema_version: u32) -> Self {
        Self::new(
            MessageType::Error,
            schema_version,
            MessagePayload::Error { code, message },
        )
    }
}
