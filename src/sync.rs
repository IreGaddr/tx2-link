use crate::error::{LinkError, Result};
use crate::protocol::*;
use crate::serialization::{WorldSnapshot, Delta};
use crate::transport::Transport;
use crate::compression::DeltaCompressor;
use crate::rate_limit::{RateLimiter, RateLimitConfig};
use crate::schema::{SchemaRegistry, SchemaVersion};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMode {
    Full,
    Delta,
    Manual,
}

#[derive(Debug, Clone)]
pub struct SyncConfig {
    pub mode: SyncMode,
    pub sync_interval: Duration,
    pub enable_rate_limiting: bool,
    pub rate_limit_config: RateLimitConfig,
    pub enable_field_compression: bool,
    pub auto_reconnect: bool,
    pub max_reconnect_attempts: u32,
    pub reconnect_delay: Duration,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            mode: SyncMode::Delta,
            sync_interval: Duration::from_millis(100),
            enable_rate_limiting: true,
            rate_limit_config: RateLimitConfig::default(),
            enable_field_compression: true,
            auto_reconnect: false,
            max_reconnect_attempts: 3,
            reconnect_delay: Duration::from_secs(1),
        }
    }
}

impl SyncConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_mode(mut self, mode: SyncMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_sync_interval(mut self, interval: Duration) -> Self {
        self.sync_interval = interval;
        self
    }

    pub fn with_rate_limiting(mut self, enabled: bool) -> Self {
        self.enable_rate_limiting = enabled;
        self
    }

    pub fn with_rate_limit_config(mut self, config: RateLimitConfig) -> Self {
        self.rate_limit_config = config;
        self
    }

    pub fn with_field_compression(mut self, enabled: bool) -> Self {
        self.enable_field_compression = enabled;
        self
    }

    pub fn with_auto_reconnect(mut self, enabled: bool, max_attempts: u32) -> Self {
        self.auto_reconnect = enabled;
        self.max_reconnect_attempts = max_attempts;
        self
    }
}

pub struct SyncManager<T: Transport> {
    transport: T,
    config: SyncConfig,
    delta_compressor: DeltaCompressor,
    rate_limiter: Option<RateLimiter>,
    schema_registry: SchemaRegistry,
    last_sync: Option<Instant>,
    sync_count: u64,
    error_count: u64,
    reconnect_attempts: u32,
    schema_version: SchemaVersion,
}

impl<T: Transport> SyncManager<T> {
    pub fn new(transport: T, config: SyncConfig) -> Self {
        let delta_compressor = DeltaCompressor::with_field_compression(config.enable_field_compression);
        let rate_limiter = if config.enable_rate_limiting {
            Some(RateLimiter::new(config.rate_limit_config.clone()))
        } else {
            None
        };

        Self {
            transport,
            config,
            delta_compressor,
            rate_limiter,
            schema_registry: SchemaRegistry::new(),
            last_sync: None,
            sync_count: 0,
            error_count: 0,
            reconnect_attempts: 0,
            schema_version: 1,
        }
    }

    pub fn send_snapshot(&mut self, snapshot: WorldSnapshot) -> Result<()> {
        if !self.transport.is_connected() {
            if self.config.auto_reconnect && self.reconnect_attempts < self.config.max_reconnect_attempts {
                self.reconnect_attempts += 1;
                return Err(LinkError::ConnectionClosed);
            } else {
                return Err(LinkError::ConnectionClosed);
            }
        }

        let schema_version = self.schema_version;
        let message = Message::snapshot(
            snapshot.entities,
            snapshot.timestamp,
            schema_version,
        );

        let estimated_size = 1024u64;
        if let Some(limiter) = &mut self.rate_limiter {
            limiter.check_and_record(estimated_size)?;
        }

        self.transport.send(&message)?;

        self.last_sync = Some(Instant::now());
        self.sync_count += 1;
        self.reconnect_attempts = 0;

        Ok(())
    }

    pub fn send_delta(&mut self, snapshot: WorldSnapshot) -> Result<()> {
        if !self.transport.is_connected() {
            if self.config.auto_reconnect && self.reconnect_attempts < self.config.max_reconnect_attempts {
                self.reconnect_attempts += 1;
                return Err(LinkError::ConnectionClosed);
            } else {
                return Err(LinkError::ConnectionClosed);
            }
        }

        let delta = self.delta_compressor.create_delta(snapshot);

        if delta.changes.is_empty() {
            return Ok(());
        }

        let base_timestamp = (delta.base_timestamp * 1000.0) as u64;
        let schema_version = self.schema_version;
        let message = Message::delta(delta.changes, base_timestamp, schema_version);

        let estimated_size = 1024u64;
        if let Some(limiter) = &mut self.rate_limiter {
            limiter.check_and_record(estimated_size)?;
        }

        self.transport.send(&message)?;

        self.last_sync = Some(Instant::now());
        self.sync_count += 1;
        self.reconnect_attempts = 0;

        Ok(())
    }

    pub fn send(&mut self, snapshot: WorldSnapshot) -> Result<()> {
        match self.config.mode {
            SyncMode::Full => self.send_snapshot(snapshot),
            SyncMode::Delta => self.send_delta(snapshot),
            SyncMode::Manual => Ok(()),
        }
    }

    pub fn receive(&mut self) -> Result<Option<SyncEvent>> {
        if !self.transport.is_connected() {
            return Err(LinkError::ConnectionClosed);
        }

        match self.transport.receive()? {
            Some(message) => {
                let event = self.process_message(message)?;
                Ok(Some(event))
            }
            None => Ok(None),
        }
    }

    fn process_message(&mut self, message: Message) -> Result<SyncEvent> {
        match message.payload {
            MessagePayload::Snapshot(payload) => {
                let snapshot = WorldSnapshot {
                    entities: payload.entities,
                    timestamp: payload.metadata.world_time,
                    version: "1.0.0".to_string(),
                };

                self.delta_compressor.reset();

                Ok(SyncEvent::Snapshot(snapshot))
            }
            MessagePayload::Delta(payload) => {
                let delta = Delta {
                    changes: payload.changes,
                    timestamp: message.header.timestamp as f64 / 1000.0,
                    base_timestamp: payload.base_timestamp as f64 / 1000.0,
                };

                Ok(SyncEvent::Delta(delta))
            }
            MessagePayload::RequestSnapshot => {
                Ok(SyncEvent::SnapshotRequested)
            }
            MessagePayload::Ack { ack_id } => {
                Ok(SyncEvent::Ack(ack_id))
            }
            MessagePayload::Ping => {
                let pong = Message::pong(self.schema_version);
                self.transport.send(&pong)?;
                Ok(SyncEvent::Ping)
            }
            MessagePayload::Pong => {
                Ok(SyncEvent::Pong)
            }
            MessagePayload::SchemaSync(payload) => {
                Ok(SyncEvent::SchemaSync(payload.schemas))
            }
            MessagePayload::Error { code, message: error_message } => {
                self.error_count += 1;
                Ok(SyncEvent::Error { code, message: error_message })
            }
        }
    }

    pub fn request_snapshot(&mut self) -> Result<()> {
        let message = Message::request_snapshot(self.schema_version);
        self.transport.send(&message)?;
        Ok(())
    }

    pub fn send_ack(&mut self, message_id: u64) -> Result<()> {
        let message = Message::ack(message_id, self.schema_version);
        self.transport.send(&message)?;
        Ok(())
    }

    pub fn ping(&mut self) -> Result<()> {
        let message = Message::ping(self.schema_version);
        self.transport.send(&message)?;
        Ok(())
    }

    pub fn should_sync(&self) -> bool {
        if self.config.mode == SyncMode::Manual {
            return false;
        }

        if let Some(last_sync) = self.last_sync {
            last_sync.elapsed() >= self.config.sync_interval
        } else {
            true
        }
    }

    pub fn get_stats(&self) -> SyncStats {
        let rate_limiter_stats = self.rate_limiter.as_ref().map(|l| l.get_stats());

        SyncStats {
            sync_count: self.sync_count,
            error_count: self.error_count,
            last_sync: self.last_sync,
            rate_limiter_stats,
            reconnect_attempts: self.reconnect_attempts,
        }
    }

    pub fn get_schema_registry(&self) -> &SchemaRegistry {
        &self.schema_registry
    }

    pub fn get_schema_registry_mut(&mut self) -> &mut SchemaRegistry {
        &mut self.schema_registry
    }

    pub fn set_schema_version(&mut self, version: SchemaVersion) {
        self.schema_version = version;
    }

    pub fn get_schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    pub fn reset_delta_compressor(&mut self) {
        self.delta_compressor.reset();
    }

    pub fn is_connected(&self) -> bool {
        self.transport.is_connected()
    }

    pub fn close(&mut self) -> Result<()> {
        self.transport.close()
    }

    fn estimate_message_size(&self, _message: &Message) -> u64 {
        1024
    }
}

#[derive(Debug, Clone)]
pub struct SyncStats {
    pub sync_count: u64,
    pub error_count: u64,
    pub last_sync: Option<Instant>,
    pub rate_limiter_stats: Option<crate::rate_limit::RateLimitStats>,
    pub reconnect_attempts: u32,
}

#[derive(Debug)]
pub enum SyncEvent {
    Snapshot(WorldSnapshot),
    Delta(Delta),
    SnapshotRequested,
    Ack(u64),
    Ping,
    Pong,
    SchemaSync(Vec<ComponentSchemaInfo>),
    Error { code: u32, message: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MemoryTransport;
    use crate::serialization::BinaryFormat;

    #[test]
    fn test_sync_manager_snapshot() {
        let transport = MemoryTransport::new(BinaryFormat::MessagePack);
        let config = SyncConfig::new().with_mode(SyncMode::Full);
        let mut manager = SyncManager::new(transport, config);

        let snapshot = WorldSnapshot {
            entities: vec![],
            timestamp: 100.0,
            version: "1.0.0".to_string(),
        };

        assert!(manager.send_snapshot(snapshot).is_ok());
        assert_eq!(manager.get_stats().sync_count, 1);
    }

    #[test]
    fn test_sync_manager_delta() {
        use crate::protocol::{SerializedEntity, SerializedComponent, ComponentData};

        let transport = MemoryTransport::new(BinaryFormat::MessagePack);
        let config = SyncConfig::new().with_mode(SyncMode::Delta);
        let mut manager = SyncManager::new(transport, config);

        let snapshot1 = WorldSnapshot {
            entities: vec![],
            timestamp: 100.0,
            version: "1.0.0".to_string(),
        };

        assert!(manager.send_delta(snapshot1).is_ok());

        let snapshot2 = WorldSnapshot {
            entities: vec![
                SerializedEntity {
                    id: 1,
                    components: vec![
                        SerializedComponent {
                            id: "Position".to_string(),
                            data: ComponentData::from_json_value(serde_json::json!({"x": 10.0})),
                        }
                    ],
                }
            ],
            timestamp: 200.0,
            version: "1.0.0".to_string(),
        };

        assert!(manager.send_delta(snapshot2).is_ok());
        assert_eq!(manager.get_stats().sync_count, 1);
    }

    #[test]
    fn test_sync_manager_rate_limiting() {
        let transport = MemoryTransport::new(BinaryFormat::MessagePack);
        let rate_config = RateLimitConfig::new().with_max_messages(2);
        let config = SyncConfig::new()
            .with_mode(SyncMode::Full)
            .with_rate_limiting(true)
            .with_rate_limit_config(rate_config);

        let mut manager = SyncManager::new(transport, config);

        let snapshot = WorldSnapshot {
            entities: vec![],
            timestamp: 100.0,
            version: "1.0.0".to_string(),
        };

        assert!(manager.send_snapshot(snapshot.clone()).is_ok());
        assert!(manager.send_snapshot(snapshot.clone()).is_ok());
        assert!(manager.send_snapshot(snapshot).is_err());
    }
}
