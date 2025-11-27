use crate::error::Result;
use crate::protocol::*;
use serde::{Deserialize, Serialize};
use bytes::{Bytes, BytesMut, BufMut};

pub use crate::protocol::{SerializedComponent, SerializedEntity};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub entities: Vec<SerializedEntity>,
    pub timestamp: f64,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delta {
    pub changes: Vec<DeltaChange>,
    pub timestamp: f64,
    pub base_timestamp: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryFormat {
    Json,
    MessagePack,
    Bincode,
}

pub struct BinarySerializer {
    format: BinaryFormat,
}

impl BinarySerializer {
    pub fn new(format: BinaryFormat) -> Self {
        Self { format }
    }

    pub fn json() -> Self {
        Self::new(BinaryFormat::Json)
    }

    pub fn messagepack() -> Self {
        Self::new(BinaryFormat::MessagePack)
    }

    pub fn bincode() -> Self {
        Self::new(BinaryFormat::Bincode)
    }

    pub fn serialize_message(&self, message: &Message) -> Result<Bytes> {
        match self.format {
            BinaryFormat::Json => {
                let json = serde_json::to_vec(message)?;
                Ok(Bytes::from(json))
            }
            BinaryFormat::MessagePack => {
                let msgpack = rmp_serde::to_vec(message)?;
                Ok(Bytes::from(msgpack))
            }
            BinaryFormat::Bincode => {
                let bincode_data = bincode::serialize(message)?;
                Ok(Bytes::from(bincode_data))
            }
        }
    }

    pub fn deserialize_message(&self, data: &[u8]) -> Result<Message> {
        match self.format {
            BinaryFormat::Json => {
                let message = serde_json::from_slice(data)?;
                Ok(message)
            }
            BinaryFormat::MessagePack => {
                let message = rmp_serde::from_slice(data)?;
                Ok(message)
            }
            BinaryFormat::Bincode => {
                let message = bincode::deserialize(data)?;
                Ok(message)
            }
        }
    }

    pub fn serialize_snapshot(&self, snapshot: &WorldSnapshot) -> Result<Bytes> {
        match self.format {
            BinaryFormat::Json => {
                let json = serde_json::to_vec(snapshot)?;
                Ok(Bytes::from(json))
            }
            BinaryFormat::MessagePack => {
                let msgpack = rmp_serde::to_vec(snapshot)?;
                Ok(Bytes::from(msgpack))
            }
            BinaryFormat::Bincode => {
                let bincode_data = bincode::serialize(snapshot)?;
                Ok(Bytes::from(bincode_data))
            }
        }
    }

    pub fn deserialize_snapshot(&self, data: &[u8]) -> Result<WorldSnapshot> {
        match self.format {
            BinaryFormat::Json => {
                let snapshot = serde_json::from_slice(data)?;
                Ok(snapshot)
            }
            BinaryFormat::MessagePack => {
                let snapshot = rmp_serde::from_slice(data)?;
                Ok(snapshot)
            }
            BinaryFormat::Bincode => {
                let snapshot = bincode::deserialize(data)?;
                Ok(snapshot)
            }
        }
    }

    pub fn serialize_delta(&self, delta: &Delta) -> Result<Bytes> {
        match self.format {
            BinaryFormat::Json => {
                let json = serde_json::to_vec(delta)?;
                Ok(Bytes::from(json))
            }
            BinaryFormat::MessagePack => {
                let msgpack = rmp_serde::to_vec(delta)?;
                Ok(Bytes::from(msgpack))
            }
            BinaryFormat::Bincode => {
                let bincode_data = bincode::serialize(delta)?;
                Ok(Bytes::from(bincode_data))
            }
        }
    }

    pub fn deserialize_delta(&self, data: &[u8]) -> Result<Delta> {
        match self.format {
            BinaryFormat::Json => {
                let delta = serde_json::from_slice(data)?;
                Ok(delta)
            }
            BinaryFormat::MessagePack => {
                let delta = rmp_serde::from_slice(data)?;
                Ok(delta)
            }
            BinaryFormat::Bincode => {
                let delta = bincode::deserialize(data)?;
                Ok(delta)
            }
        }
    }

    pub fn serialize_component(&self, component: &SerializedComponent) -> Result<Bytes> {
        match self.format {
            BinaryFormat::Json => {
                let json = serde_json::to_vec(component)?;
                Ok(Bytes::from(json))
            }
            BinaryFormat::MessagePack => {
                let msgpack = rmp_serde::to_vec(component)?;
                Ok(Bytes::from(msgpack))
            }
            BinaryFormat::Bincode => {
                let bincode_data = bincode::serialize(component)?;
                Ok(Bytes::from(bincode_data))
            }
        }
    }

    pub fn deserialize_component(&self, data: &[u8]) -> Result<SerializedComponent> {
        match self.format {
            BinaryFormat::Json => {
                let component = serde_json::from_slice(data)?;
                Ok(component)
            }
            BinaryFormat::MessagePack => {
                let component = rmp_serde::from_slice(data)?;
                Ok(component)
            }
            BinaryFormat::Bincode => {
                let component = bincode::deserialize(data)?;
                Ok(component)
            }
        }
    }

    pub fn get_format(&self) -> BinaryFormat {
        self.format
    }
}

pub struct StreamingSerializer {
    format: BinaryFormat,
    buffer: BytesMut,
}

impl StreamingSerializer {
    pub fn new(format: BinaryFormat) -> Self {
        Self {
            format,
            buffer: BytesMut::with_capacity(8192),
        }
    }

    pub fn write_message(&mut self, message: &Message) -> Result<()> {
        let serializer = BinarySerializer::new(self.format);
        let data = serializer.serialize_message(message)?;

        let len = data.len() as u32;
        self.buffer.put_u32_le(len);
        self.buffer.put(data);

        Ok(())
    }

    pub fn flush(&mut self) -> Bytes {
        self.buffer.split().freeze()
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

pub struct StreamingDeserializer {
    format: BinaryFormat,
    buffer: BytesMut,
}

impl StreamingDeserializer {
    pub fn new(format: BinaryFormat) -> Self {
        Self {
            format,
            buffer: BytesMut::with_capacity(8192),
        }
    }

    pub fn feed(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    pub fn try_read_message(&mut self) -> Result<Option<Message>> {
        if self.buffer.len() < 4 {
            return Ok(None);
        }

        let len = u32::from_le_bytes([
            self.buffer[0],
            self.buffer[1],
            self.buffer[2],
            self.buffer[3],
        ]) as usize;

        if self.buffer.len() < 4 + len {
            return Ok(None);
        }

        self.buffer.advance(4);

        let message_data = self.buffer.split_to(len);

        let serializer = BinarySerializer::new(self.format);
        let message = serializer.deserialize_message(&message_data)?;

        Ok(Some(message))
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

trait Advance {
    fn advance(&mut self, cnt: usize);
}

impl Advance for BytesMut {
    fn advance(&mut self, cnt: usize) {
        let _ = self.split_to(cnt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_serialization() {
        let serializer = BinarySerializer::json();
        let message = Message::ping(1);

        let serialized = serializer.serialize_message(&message).unwrap();
        let deserialized = serializer.deserialize_message(&serialized).unwrap();

        assert_eq!(message.header.msg_type, deserialized.header.msg_type);
    }

    #[test]
    fn test_messagepack_serialization() {
        let serializer = BinarySerializer::messagepack();
        let message = Message::ping(1);

        let serialized = serializer.serialize_message(&message).unwrap();
        let deserialized = serializer.deserialize_message(&serialized).unwrap();

        assert_eq!(message.header.msg_type, deserialized.header.msg_type);
    }

    #[test]
    fn test_bincode_serialization() {
        let serializer = BinarySerializer::bincode();

        let snapshot = WorldSnapshot {
            entities: vec![],
            timestamp: 100.0,
            version: "1.0.0".to_string(),
        };

        let serialized = serializer.serialize_snapshot(&snapshot).unwrap();
        let deserialized = serializer.deserialize_snapshot(&serialized).unwrap();

        assert_eq!(snapshot.timestamp, deserialized.timestamp);
    }

    #[test]
    fn test_streaming_serialization() {
        let mut stream_serializer = StreamingSerializer::new(BinaryFormat::MessagePack);
        let mut stream_deserializer = StreamingDeserializer::new(BinaryFormat::MessagePack);

        let msg1 = Message::ping(1);
        let msg2 = Message::pong(1);

        stream_serializer.write_message(&msg1).unwrap();
        stream_serializer.write_message(&msg2).unwrap();

        let data = stream_serializer.flush();
        stream_deserializer.feed(&data);

        let decoded1 = stream_deserializer.try_read_message().unwrap().unwrap();
        let decoded2 = stream_deserializer.try_read_message().unwrap().unwrap();

        assert_eq!(msg1.header.msg_type, decoded1.header.msg_type);
        assert_eq!(msg2.header.msg_type, decoded2.header.msg_type);
    }

    #[test]
    fn test_snapshot_serialization() {
        let snapshot = WorldSnapshot {
            entities: vec![
                SerializedEntity {
                    id: 1,
                    components: vec![
                        SerializedComponent {
                            id: "Position".to_string(),
                            data: ComponentData::from_json_value(serde_json::json!({"x": 10.0, "y": 20.0})),
                        }
                    ],
                }
            ],
            timestamp: 123.456,
            version: "1.0.0".to_string(),
        };

        let serializer = BinarySerializer::messagepack();
        let serialized = serializer.serialize_snapshot(&snapshot).unwrap();
        let deserialized = serializer.deserialize_snapshot(&serialized).unwrap();

        assert_eq!(snapshot.entities.len(), deserialized.entities.len());
        assert_eq!(snapshot.timestamp, deserialized.timestamp);
        assert_eq!(snapshot.version, deserialized.version);
    }
}
