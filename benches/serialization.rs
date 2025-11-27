use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use tx2_link::{
    BinarySerializer, BinaryFormat,
    WorldSnapshot, SerializedEntity, SerializedComponent,
    protocol::{Message, ComponentData, FieldValue},
    compression::DeltaCompressor,
};
use std::collections::HashMap;

fn create_test_snapshot(entity_count: usize, components_per_entity: usize) -> WorldSnapshot {
    let mut entities = Vec::with_capacity(entity_count);

    for i in 0..entity_count {
        let mut components = Vec::with_capacity(components_per_entity);

        for j in 0..components_per_entity {
            let mut fields = HashMap::new();
            fields.insert("x".to_string(), FieldValue::F64((i * j) as f64));
            fields.insert("y".to_string(), FieldValue::F64((i + j) as f64));
            fields.insert("z".to_string(), FieldValue::F64((i - j) as f64));
            fields.insert("name".to_string(), FieldValue::String(format!("Entity_{}_Component_{}", i, j)));
            fields.insert("active".to_string(), FieldValue::Bool(i % 2 == 0));

            components.push(SerializedComponent {
                id: format!("Component{}", j),
                data: ComponentData::Structured(fields),
            });
        }

        entities.push(SerializedEntity {
            id: i as u32,
            components,
        });
    }

    WorldSnapshot {
        entities,
        timestamp: 100.0,
        version: "1.0.0".to_string(),
    }
}

fn benchmark_serialization_formats(c: &mut Criterion) {
    let snapshot = create_test_snapshot(100, 5);

    let mut group = c.benchmark_group("serialization_formats");

    for format in &[BinaryFormat::Json, BinaryFormat::MessagePack, BinaryFormat::Bincode] {
        let format_name = match format {
            BinaryFormat::Json => "JSON",
            BinaryFormat::MessagePack => "MessagePack",
            BinaryFormat::Bincode => "Bincode",
        };

        group.bench_with_input(
            BenchmarkId::new("serialize_snapshot", format_name),
            format,
            |b, format| {
                let serializer = BinarySerializer::new(*format);
                b.iter(|| {
                    black_box(serializer.serialize_snapshot(&snapshot).unwrap());
                });
            },
        );
    }

    group.finish();
}

fn benchmark_deserialization_formats(c: &mut Criterion) {
    let snapshot = create_test_snapshot(100, 5);

    let mut group = c.benchmark_group("deserialization_formats");

    let json_serializer = BinarySerializer::json();
    let json_data = json_serializer.serialize_snapshot(&snapshot).unwrap();

    group.bench_function("deserialize_snapshot/JSON", |b| {
        b.iter(|| {
            black_box(json_serializer.deserialize_snapshot(&json_data).unwrap());
        });
    });

    let msgpack_serializer = BinarySerializer::messagepack();
    let msgpack_data = msgpack_serializer.serialize_snapshot(&snapshot).unwrap();

    group.bench_function("deserialize_snapshot/MessagePack", |b| {
        b.iter(|| {
            black_box(msgpack_serializer.deserialize_snapshot(&msgpack_data).unwrap());
        });
    });

    let bincode_serializer = BinarySerializer::bincode();
    let bincode_data = bincode_serializer.serialize_snapshot(&snapshot).unwrap();

    group.bench_function("deserialize_snapshot/Bincode", |b| {
        b.iter(|| {
            black_box(bincode_serializer.deserialize_snapshot(&bincode_data).unwrap());
        });
    });

    group.finish();
}

fn benchmark_message_sizes(c: &mut Criterion) {
    let snapshot = create_test_snapshot(100, 5);

    let mut group = c.benchmark_group("message_sizes");

    for format in &[BinaryFormat::Json, BinaryFormat::MessagePack, BinaryFormat::Bincode] {
        let format_name = match format {
            BinaryFormat::Json => "JSON",
            BinaryFormat::MessagePack => "MessagePack",
            BinaryFormat::Bincode => "Bincode",
        };

        let serializer = BinarySerializer::new(*format);
        let serialized = serializer.serialize_snapshot(&snapshot).unwrap();

        println!("{} size: {} bytes", format_name, serialized.len());
    }

    group.finish();
}

fn benchmark_delta_compression(c: &mut Criterion) {
    let snapshot1 = create_test_snapshot(100, 5);
    let mut snapshot2 = create_test_snapshot(100, 5);

    snapshot2.entities[0].components[0] = SerializedComponent {
        id: "Position".to_string(),
        data: ComponentData::Structured({
            let mut fields = HashMap::new();
            fields.insert("x".to_string(), FieldValue::F64(999.0));
            fields.insert("y".to_string(), FieldValue::F64(999.0));
            fields
        }),
    };

    c.bench_function("delta_compression", |b| {
        b.iter(|| {
            let mut compressor = DeltaCompressor::new();
            compressor.create_delta(black_box(snapshot1.clone()));
            black_box(compressor.create_delta(black_box(snapshot2.clone())));
        });
    });
}

fn benchmark_delta_compression_field_level(c: &mut Criterion) {
    let snapshot1 = create_test_snapshot(100, 5);
    let mut snapshot2 = create_test_snapshot(100, 5);

    snapshot2.entities[0].components[0] = SerializedComponent {
        id: "Position".to_string(),
        data: ComponentData::Structured({
            let mut fields = HashMap::new();
            fields.insert("x".to_string(), FieldValue::F64(999.0));
            fields.insert("y".to_string(), FieldValue::F64(999.0));
            fields
        }),
    };

    c.bench_function("delta_compression_with_field_level", |b| {
        b.iter(|| {
            let mut compressor = DeltaCompressor::with_field_compression(true);
            compressor.create_delta(black_box(snapshot1.clone()));
            black_box(compressor.create_delta(black_box(snapshot2.clone())));
        });
    });
}

fn benchmark_snapshot_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot_size_scaling");

    for entity_count in &[10, 50, 100, 500, 1000] {
        group.throughput(Throughput::Elements(*entity_count as u64));

        let snapshot = create_test_snapshot(*entity_count, 5);

        group.bench_with_input(
            BenchmarkId::new("messagepack", entity_count),
            entity_count,
            |b, _| {
                let serializer = BinarySerializer::messagepack();
                b.iter(|| {
                    black_box(serializer.serialize_snapshot(&snapshot).unwrap());
                });
            },
        );
    }

    group.finish();
}

fn benchmark_message_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_serialization");

    let message = Message::ping(1);

    for format in &[BinaryFormat::Json, BinaryFormat::MessagePack, BinaryFormat::Bincode] {
        let format_name = match format {
            BinaryFormat::Json => "JSON",
            BinaryFormat::MessagePack => "MessagePack",
            BinaryFormat::Bincode => "Bincode",
        };

        group.bench_with_input(
            BenchmarkId::new("serialize_message", format_name),
            format,
            |b, format| {
                let serializer = BinarySerializer::new(*format);
                b.iter(|| {
                    black_box(serializer.serialize_message(&message).unwrap());
                });
            },
        );
    }

    group.finish();
}

fn benchmark_delta_size_comparison(c: &mut Criterion) {
    let snapshot1 = create_test_snapshot(1000, 10);
    let mut snapshot2 = snapshot1.clone();

    for i in 0..10 {
        snapshot2.entities[i].components[0] = SerializedComponent {
            id: "Position".to_string(),
            data: ComponentData::Structured({
                let mut fields = HashMap::new();
                fields.insert("x".to_string(), FieldValue::F64((i * 100) as f64));
                fields.insert("y".to_string(), FieldValue::F64((i * 100) as f64));
                fields
            }),
        };
    }

    println!("\n=== Delta Size Comparison (1000 entities, 10 changed) ===");

    let mut compressor_no_field = DeltaCompressor::with_field_compression(false);
    compressor_no_field.create_delta(snapshot1.clone());
    let delta_no_field = compressor_no_field.create_delta(snapshot2.clone());

    let mut compressor_field = DeltaCompressor::with_field_compression(true);
    compressor_field.create_delta(snapshot1.clone());
    let delta_field = compressor_field.create_delta(snapshot2.clone());

    let serializer = BinarySerializer::messagepack();

    let full_size = serializer.serialize_snapshot(&snapshot2).unwrap().len();
    let delta_no_field_size = serializer.serialize_delta(&delta_no_field).unwrap().len();
    let delta_field_size = serializer.serialize_delta(&delta_field).unwrap().len();

    println!("Full snapshot size: {} bytes", full_size);
    println!("Delta (component-level) size: {} bytes ({:.2}% of full)",
             delta_no_field_size,
             (delta_no_field_size as f64 / full_size as f64) * 100.0);
    println!("Delta (field-level) size: {} bytes ({:.2}% of full)",
             delta_field_size,
             (delta_field_size as f64 / full_size as f64) * 100.0);
    println!("Field-level compression improvement: {:.2}%\n",
             ((delta_no_field_size as f64 - delta_field_size as f64) / delta_no_field_size as f64) * 100.0);

    c.bench_function("delta_size_comparison", |b| {
        b.iter(|| {
            black_box(&delta_field);
        });
    });
}

criterion_group!(
    benches,
    benchmark_serialization_formats,
    benchmark_deserialization_formats,
    benchmark_message_sizes,
    benchmark_delta_compression,
    benchmark_delta_compression_field_level,
    benchmark_snapshot_sizes,
    benchmark_message_serialization,
    benchmark_delta_size_comparison,
);

criterion_main!(benches);
