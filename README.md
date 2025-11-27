# tx2-link

**Binary protocol for syncing ECS worlds between runtimes with field-level delta compression.**

tx2-link is the bridge/protocol layer of the TX-2 ecosystem, enabling efficient synchronization of Entity-Component-System state across web, native, and CLI environments. It defines the "wire format" for transmitting world snapshots and deltas with minimal bandwidth overhead.

## Features

### Delta Compression
- **Field-level change detection** - Only transmit changed component fields
- **1171× compression ratio** - Benchmarked: 1.2MB full snapshot → 1KB delta (10% entity churn)
- **Automatic delta generation** - Compare snapshots and extract minimal diff
- **Delta application** - Reconstruct full state from base + delta

### Multiple Serialization Formats
- **MessagePack** - Compact binary format (default, best compression)
- **Bincode** - Fast Rust-native serialization (lowest latency)
- **JSON** - Human-readable debugging format

### Transport Abstractions
- **WebSocket** - Server ↔ browser sync (async)
- **IPC** - Inter-process communication for native ↔ webview
- **Stdio** - Pipe-based communication for CLI tools
- **Memory** - In-process channels for testing

### Rate Limiting
- **Token bucket** - Burst handling with sustained rate limits
- **Sliding window** - Precise message/byte rate enforcement
- **Per-connection limits** - Individual rate limiters per client
- **357k checks/sec** - Benchmarked throughput while enforcing 1k msg/sec cap

### Schema Versioning
- **Component schema registry** - Type definitions with version tracking
- **Schema validation** - Ensure client/server compatibility
- **Migration support** - Handle schema evolution gracefully

### Streaming Protocol
- **Length-prefixed framing** - Parse messages from byte streams
- **Zero-copy deserialization** - Direct memory mapping where possible
- **Backpressure support** - Flow control for slow consumers

## Quick Start

### Delta Compression

```rust
use tx2_link::{WorldSnapshot, DeltaCompressor};

let mut compressor = DeltaCompressor::new();

// Create two snapshots
let snapshot1 = WorldSnapshot { /* ... */ };
let snapshot2 = WorldSnapshot { /* ... */ };

// Generate delta (only changed fields)
let delta = compressor.create_delta(&snapshot1, &snapshot2)?;

// Apply delta to reconstruct snapshot2
let reconstructed = compressor.apply_delta(&snapshot1, &delta)?;

assert_eq!(snapshot2, reconstructed);
```

### Serialization

```rust
use tx2_link::{Message, Serializer, SerializationFormat};

// Create message
let message = Message::Snapshot(snapshot);

// Serialize to MessagePack
let mut serializer = Serializer::new(SerializationFormat::MessagePack);
let bytes = serializer.serialize(&message)?;

// Deserialize
let deserialized: Message = serializer.deserialize(&bytes)?;
```

### Transport

```rust
use tx2_link::{Transport, WebSocketTransport};

// Create WebSocket transport
let transport = WebSocketTransport::connect("ws://localhost:8080").await?;

// Send message
transport.send(&message).await?;

// Receive message
let received = transport.receive().await?;
```

### Rate Limiting

```rust
use tx2_link::{RateLimiter, TokenBucketLimiter};

// Create rate limiter: 100 msg/sec, burst of 10
let mut limiter = TokenBucketLimiter::new(100.0, 10);

// Check if message can be sent
if limiter.check_message(1)? {
    transport.send(&message).await?;
}
```

## Performance

Benchmarked on 10,000 entities with Position, Velocity, Health components:

### Delta Compression
- **Full snapshot**: 1,232,875 bytes
- **Delta (10% churn)**: 1,052 bytes
- **Compression ratio**: 1171×
- **Delta generation**: ~2ms
- **Delta application**: ~1.5ms

### Serialization Performance
| Format | Serialize | Deserialize | Size |
|--------|-----------|-------------|------|
| MessagePack | 180µs | 250µs | 1.05MB |
| Bincode | 140µs | 195µs | 1.12MB |
| JSON | 420µs | 350µs | 2.28MB |

### Rate Limiting
- **Check throughput**: 357k checks/sec
- **Overhead**: ~3µs per check
- **Memory**: ~200 bytes per limiter

## Architecture

### Protocol Messages

```rust
pub enum Message {
    Snapshot(WorldSnapshot),          // Full world state
    Delta(DeltaSnapshot),             // Incremental update
    EntityCreated { id, components }, // New entity
    EntityDeleted { id },             // Entity removed
    ComponentAdded { entity, data },  // Component attached
    ComponentRemoved { entity, id },  // Component detached
    SchemaUpdate(ComponentSchema),    // Type definition
}
```

### World Snapshot

```rust
pub struct WorldSnapshot {
    pub timestamp: u64,
    pub entities: Vec<EntitySnapshot>,
}

pub struct EntitySnapshot {
    pub id: EntityId,
    pub components: Vec<ComponentSnapshot>,
}

pub struct ComponentSnapshot {
    pub id: ComponentId,
    pub data: ComponentData,
}
```

### Component Data Formats

```rust
pub enum ComponentData {
    Binary(Vec<u8>),                           // Raw bytes
    Json(String),                              // JSON string
    Structured(HashMap<FieldId, FieldValue>),  // Field-level access
}
```

## Delta Algorithm

tx2-link uses field-level diffing for maximum compression:

1. **Compare entities** - Match entities between snapshots by ID
2. **Detect additions/removals** - Track created/deleted entities
3. **Compare components** - Match components by type within each entity
4. **Field-level diff** - Extract only changed fields within components
5. **Generate delta** - Encode minimal changeset

For structured components:
```rust
// Previous: { x: 10.0, y: 20.0, z: 30.0 }
// Current:  { x: 10.0, y: 25.0, z: 30.0 }
// Delta:    { y: 25.0 }  // Only y changed
```

## Transport Layer

All transports implement the `Transport` trait:

```rust
#[async_trait]
pub trait Transport: Send + Sync {
    async fn send(&self, message: &Message) -> Result<()>;
    async fn receive(&self) -> Result<Message>;
    async fn close(&self) -> Result<()>;
}
```

### WebSocket Transport

```rust
use tx2_link::WebSocketTransport;

// Server
let transport = WebSocketTransport::bind("127.0.0.1:8080").await?;

// Client
let transport = WebSocketTransport::connect("ws://localhost:8080").await?;
```

### IPC Transport

```rust
use tx2_link::IpcTransport;

// Create named pipe/socket
let transport = IpcTransport::new("tx2-world")?;
```

### Stdio Transport

```rust
use tx2_link::StdioTransport;

// Use stdin/stdout
let transport = StdioTransport::new();
```

### Memory Transport

```rust
use tx2_link::MemoryTransport;

// In-process channels (for testing)
let (tx, rx) = MemoryTransport::create_pair();
```

## Rate Limiting

### Token Bucket

Allows bursts up to a capacity, refilling at a steady rate:

```rust
use tx2_link::TokenBucketLimiter;

// 1000 msg/sec, burst of 100
let limiter = TokenBucketLimiter::new(1000.0, 100);

// Check and consume tokens
if limiter.check_message(1)? {
    // Send message
}
```

### Sliding Window

Enforces strict limits over a time window:

```rust
use tx2_link::SlidingWindowLimiter;

// 1000 msg/sec, 1MB/sec, 60-second window
let limiter = SlidingWindowLimiter::new(
    1000,     // max messages
    1_000_000, // max bytes
    60.0,     // window seconds
);

if limiter.check(1, message_size)? {
    // Send message
}
```

## Schema Versioning

```rust
use tx2_link::{SchemaRegistry, ComponentSchema};

let mut registry = SchemaRegistry::new();

// Register component type
let schema = ComponentSchema::new("Position")
    .with_field("x", FieldType::F32)
    .with_field("y", FieldType::F32)
    .with_field("z", FieldType::F32)
    .with_version(1);

registry.register(schema)?;

// Validate incoming data
if registry.validate("Position", &component_data)? {
    // Apply update
}
```

## Integration with TX-2 Ecosystem

tx2-link bridges the TX-2 stack:

- **tx2-ecs** (TypeScript/Node): Web runtime using tx2-link for server sync
- **tx2-core** (Rust): Native engine using tx2-link for client sync
- **tx2-pack**: Uses tx2-link's snapshot format for save/load

### Use Cases

1. **Server ↔ Browser** - Sync game state over WebSocket
2. **Native ↔ Webview** - IPC between Rust engine and web UI
3. **CLI Tools** - Stream world state over stdio pipes
4. **Multi-process** - Distribute simulation across processes
5. **Debugging** - Inspect live world state with JSON transport

## Examples

### Full Client-Server Sync

```rust
use tx2_link::*;

// Server
let transport = WebSocketTransport::bind("127.0.0.1:8080").await?;
let limiter = TokenBucketLimiter::new(100.0, 10);
let mut compressor = DeltaCompressor::new();

let mut last_snapshot = world.create_snapshot();

loop {
    tokio::time::sleep(Duration::from_millis(16)).await; // 60 FPS

    let snapshot = world.create_snapshot();
    let delta = compressor.create_delta(&last_snapshot, &snapshot)?;

    if limiter.check_message(1)? {
        transport.send(&Message::Delta(delta)).await?;
    }

    last_snapshot = snapshot;
}

// Client
let transport = WebSocketTransport::connect("ws://localhost:8080").await?;
let mut compressor = DeltaCompressor::new();
let mut snapshot = WorldSnapshot::empty();

loop {
    let message = transport.receive().await?;

    match message {
        Message::Snapshot(full) => {
            snapshot = full;
            world.restore_from_snapshot(&snapshot)?;
        }
        Message::Delta(delta) => {
            snapshot = compressor.apply_delta(&snapshot, &delta)?;
            world.restore_from_snapshot(&snapshot)?;
        }
        _ => {}
    }
}
```

## Running Tests

```bash
cargo test
```

All 22 tests should pass, covering:
- Delta compression accuracy
- Serialization roundtrips (all formats)
- Rate limiter behavior
- Schema validation
- Transport abstractions

## Running Benchmarks

```bash
cargo bench
```

Benchmarks measure:
- Delta compression performance
- Serialization/deserialization speed
- Rate limiter throughput
- Field-level diff overhead

## Development Status

- [x] Core protocol messages
- [x] Delta compression with field-level diffing
- [x] Multi-format serialization (MessagePack, Bincode, JSON)
- [x] Transport abstractions (WebSocket, IPC, stdio, memory)
- [x] Rate limiting (token bucket, sliding window)
- [x] Schema versioning and validation
- [x] Streaming serializer/deserializer
- [x] Comprehensive benchmarks
- [ ] Compression (zstd/lz4) for large snapshots
- [ ] Encryption support
- [ ] Reconnection handling with state recovery
- [ ] Binary diff algorithms for large binary components

## Dependencies

- `serde` - Serialization framework
- `rmp-serde` - MessagePack format
- `bincode` - Bincode format
- `serde_json` - JSON format
- `tokio` - Async runtime
- `bytes` - Efficient byte buffers
- `ahash` - Fast hashing
- `thiserror` - Error handling

## License

MIT

## Contributing

Contributions are welcome! This is part of the broader TX-2 project for building isomorphic applications with a unified world model.

## Learn More

- [TX-2 Framework Outline](../frameworkoutline.md)
- [tx2-core Native Engine](../tx2-core)
- [tx2-pack Snapshot Format](../tx2-pack)
- [tx2-ecs TypeScript Runtime](https://github.com/IreGaddr/tx2-ecs)
