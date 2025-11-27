use crate::protocol::{Message, MessageType, DeltaChange};
use crate::serialization::{WorldSnapshot, Delta};
use std::sync::atomic::{AtomicBool, Ordering};
use std::env;

static DEBUG_MODE: AtomicBool = AtomicBool::new(false);
static TRACE_MODE: AtomicBool = AtomicBool::new(false);

/// Initialize debug mode from environment variables
///
/// - `TX2_DEBUG=1` or `TX2_DEBUG_JSON=1`: Enable JSON pretty-printing of all messages
/// - `TX2_TRACE=1`: Enable human-readable trace logging of operations
pub fn init_debug_mode() {
    let debug = env::var("TX2_DEBUG").is_ok()
        || env::var("TX2_DEBUG_JSON").is_ok();

    let trace = env::var("TX2_TRACE").is_ok();

    DEBUG_MODE.store(debug, Ordering::Relaxed);
    TRACE_MODE.store(trace, Ordering::Relaxed);

    if debug {
        eprintln!("[TX2-LINK] Debug mode enabled - all messages will be logged as JSON");
    }

    if trace {
        eprintln!("[TX2-LINK] Trace mode enabled - human-readable operation logs");
    }
}

/// Check if debug mode is enabled
pub fn is_debug_enabled() -> bool {
    DEBUG_MODE.load(Ordering::Relaxed)
}

/// Check if trace mode is enabled
pub fn is_trace_enabled() -> bool {
    TRACE_MODE.load(Ordering::Relaxed)
}

/// Log a message in JSON format if debug mode is enabled
pub fn log_message(direction: &str, message: &Message) {
    if !is_debug_enabled() {
        return;
    }

    match serde_json::to_string_pretty(message) {
        Ok(json) => {
            eprintln!("\n[TX2-LINK] {} Message:\n{}\n", direction, json);
        }
        Err(e) => {
            eprintln!("[TX2-LINK] Failed to serialize message to JSON: {}", e);
        }
    }
}

/// Log a world snapshot in JSON format if debug mode is enabled
pub fn log_snapshot(label: &str, snapshot: &WorldSnapshot) {
    if !is_debug_enabled() {
        return;
    }

    match serde_json::to_string_pretty(snapshot) {
        Ok(json) => {
            eprintln!("\n[TX2-LINK] {} Snapshot ({} entities):\n{}\n",
                label, snapshot.entities.len(), json);
        }
        Err(e) => {
            eprintln!("[TX2-LINK] Failed to serialize snapshot to JSON: {}", e);
        }
    }
}

/// Log a delta in JSON format if debug mode is enabled
pub fn log_delta(label: &str, delta: &Delta) {
    if !is_debug_enabled() {
        return;
    }

    match serde_json::to_string_pretty(delta) {
        Ok(json) => {
            let change_count = delta.changes.len();

            eprintln!("\n[TX2-LINK] {} Delta ({} changes):\n{}\n",
                label, change_count, json);
        }
        Err(e) => {
            eprintln!("[TX2-LINK] Failed to serialize delta to JSON: {}", e);
        }
    }
}

/// Trace a delta in human-readable format if trace mode is enabled
pub fn trace_delta(delta: &Delta) {
    if !is_trace_enabled() {
        return;
    }

    eprintln!("[TX2-LINK] Delta Summary:");
    eprintln!("  Timestamp: {} (base: {})", delta.timestamp, delta.base_timestamp);
    eprintln!("  Total changes: {}", delta.changes.len());

    let mut entities_added = 0;
    let mut entities_removed = 0;
    let mut components_added = 0;
    let mut components_removed = 0;
    let mut components_modified = 0;

    for change in &delta.changes {
        match change {
            DeltaChange::EntityAdded { .. } => entities_added += 1,
            DeltaChange::EntityRemoved { .. } => entities_removed += 1,
            DeltaChange::ComponentAdded { .. } => components_added += 1,
            DeltaChange::ComponentRemoved { .. } => components_removed += 1,
            DeltaChange::ComponentUpdated { .. } => components_modified += 1,
            DeltaChange::FieldsUpdated { .. } => components_modified += 1,
        }
    }

    if entities_added > 0 {
        eprintln!("  + {} entities added", entities_added);
    }
    if entities_removed > 0 {
        eprintln!("  - {} entities removed", entities_removed);
    }
    if components_added > 0 {
        eprintln!("  + {} components added", components_added);
    }
    if components_removed > 0 {
        eprintln!("  - {} components removed", components_removed);
    }
    if components_modified > 0 {
        eprintln!("  ~ {} components modified", components_modified);
    }

    eprintln!();
}

/// Trace a serialization operation
pub fn trace_serialization(format: &str, size_bytes: usize, duration_micros: u128) {
    if !is_trace_enabled() {
        return;
    }

    eprintln!("[TX2-LINK] Serialized {} bytes using {} in {}µs",
        size_bytes, format, duration_micros);
}

/// Trace a deserialization operation
pub fn trace_deserialization(format: &str, size_bytes: usize, duration_micros: u128) {
    if !is_trace_enabled() {
        return;
    }

    eprintln!("[TX2-LINK] Deserialized {} bytes using {} in {}µs",
        size_bytes, format, duration_micros);
}

/// Trace a delta compression operation
pub fn trace_compression(original_size: usize, delta_size: usize, duration_micros: u128) {
    if !is_trace_enabled() {
        return;
    }

    let ratio = if delta_size > 0 {
        original_size as f64 / delta_size as f64
    } else {
        0.0
    };

    eprintln!("[TX2-LINK] Delta compression: {} bytes → {} bytes ({:.2}× reduction) in {}µs",
        original_size, delta_size, ratio, duration_micros);
}

/// Trace a rate limit check
pub fn trace_rate_limit(allowed: bool, current_rate: f64, limit: f64) {
    if !is_trace_enabled() {
        return;
    }

    let status = if allowed { "ALLOWED" } else { "BLOCKED" };
    eprintln!("[TX2-LINK] Rate limit check: {} (current: {:.1}/s, limit: {:.1}/s)",
        status, current_rate, limit);
}

/// Trace a transport operation
pub fn trace_transport_send(bytes: usize, destination: &str) {
    if !is_trace_enabled() {
        return;
    }

    eprintln!("[TX2-LINK] → Sent {} bytes to {}", bytes, destination);
}

/// Trace a transport receive
pub fn trace_transport_receive(bytes: usize, source: &str) {
    if !is_trace_enabled() {
        return;
    }

    eprintln!("[TX2-LINK] ← Received {} bytes from {}", bytes, source);
}

/// Format bytes in human-readable format (KB, MB, etc.)
pub fn format_bytes(bytes: usize) -> String {
    const KB: usize = 1024;
    const MB: usize = KB * 1024;
    const GB: usize = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

/// Create a debug summary of a message
pub fn message_summary(message: &Message) -> String {
    match message.header.msg_type {
        MessageType::Snapshot => {
            format!("Snapshot (seq: {})", message.header.sequence)
        }
        MessageType::Delta => {
            format!("Delta (seq: {})", message.header.sequence)
        }
        MessageType::RequestSnapshot => {
            format!("RequestSnapshot (seq: {})", message.header.sequence)
        }
        MessageType::Ack => {
            format!("Ack (seq: {})", message.header.sequence)
        }
        MessageType::Ping => {
            format!("Ping (seq: {})", message.header.sequence)
        }
        MessageType::Pong => {
            format!("Pong (seq: {})", message.header.sequence)
        }
        MessageType::SchemaSync => {
            format!("SchemaSync (seq: {})", message.header.sequence)
        }
        MessageType::Error => {
            format!("Error (seq: {})", message.header.sequence)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 bytes");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn test_debug_mode_initialization() {
        // Should not crash without env vars
        init_debug_mode();
    }
}
