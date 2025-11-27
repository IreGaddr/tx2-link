use crate::protocol::*;
use crate::serialization::{WorldSnapshot, Delta};
use ahash::AHashMap;

pub struct DeltaCompressor {
    previous_snapshot: Option<WorldSnapshot>,
    field_compressor: FieldCompressor,
}

impl DeltaCompressor {
    pub fn new() -> Self {
        Self {
            previous_snapshot: None,
            field_compressor: FieldCompressor::new(),
        }
    }

    pub fn with_field_compression(enable: bool) -> Self {
        Self {
            previous_snapshot: None,
            field_compressor: FieldCompressor::with_enabled(enable),
        }
    }

    pub fn create_delta(&mut self, current_snapshot: WorldSnapshot) -> Delta {
        let timestamp = current_snapshot.timestamp;
        let base_timestamp = self.previous_snapshot.as_ref()
            .map(|s| s.timestamp)
            .unwrap_or(0.0);

        let changes = if let Some(prev) = &self.previous_snapshot {
            self.compute_changes(prev, &current_snapshot)
        } else {
            self.create_initial_delta(&current_snapshot)
        };

        self.previous_snapshot = Some(current_snapshot);

        Delta {
            changes,
            timestamp,
            base_timestamp,
        }
    }

    fn create_initial_delta(&self, snapshot: &WorldSnapshot) -> Vec<DeltaChange> {
        let mut changes = Vec::new();

        for entity in &snapshot.entities {
            changes.push(DeltaChange::EntityAdded {
                entity_id: entity.id,
            });

            for component in &entity.components {
                changes.push(DeltaChange::ComponentAdded {
                    entity_id: entity.id,
                    component_id: component.id.clone(),
                    data: component.data.clone(),
                });
            }
        }

        changes
    }

    fn compute_changes(&self, prev: &WorldSnapshot, curr: &WorldSnapshot) -> Vec<DeltaChange> {
        let mut changes = Vec::new();

        let prev_entities: AHashMap<EntityId, &SerializedEntity> = prev.entities.iter()
            .map(|e| (e.id, e))
            .collect();
        let curr_entities: AHashMap<EntityId, &SerializedEntity> = curr.entities.iter()
            .map(|e| (e.id, e))
            .collect();

        for (entity_id, curr_entity) in &curr_entities {
            if let Some(prev_entity) = prev_entities.get(entity_id) {
                self.compute_component_changes(*entity_id, prev_entity, curr_entity, &mut changes);
            } else {
                changes.push(DeltaChange::EntityAdded {
                    entity_id: *entity_id,
                });

                for component in &curr_entity.components {
                    changes.push(DeltaChange::ComponentAdded {
                        entity_id: *entity_id,
                        component_id: component.id.clone(),
                        data: component.data.clone(),
                    });
                }
            }
        }

        for entity_id in prev_entities.keys() {
            if !curr_entities.contains_key(entity_id) {
                changes.push(DeltaChange::EntityRemoved {
                    entity_id: *entity_id,
                });
            }
        }

        changes
    }

    fn compute_component_changes(
        &self,
        entity_id: EntityId,
        prev_entity: &SerializedEntity,
        curr_entity: &SerializedEntity,
        changes: &mut Vec<DeltaChange>,
    ) {
        let prev_components: AHashMap<&str, &SerializedComponent> = prev_entity.components.iter()
            .map(|c| (c.id.as_str(), c))
            .collect();
        let curr_components: AHashMap<&str, &SerializedComponent> = curr_entity.components.iter()
            .map(|c| (c.id.as_str(), c))
            .collect();

        for (component_id, curr_component) in &curr_components {
            if let Some(prev_component) = prev_components.get(component_id) {
                if !self.components_equal(prev_component, curr_component) {
                    if self.field_compressor.is_enabled() {
                        if let Some(field_deltas) = self.field_compressor.compute_field_deltas(
                            prev_component,
                            curr_component,
                        ) {
                            if !field_deltas.is_empty() {
                                changes.push(DeltaChange::FieldsUpdated {
                                    entity_id,
                                    component_id: component_id.to_string(),
                                    fields: field_deltas,
                                });
                                continue;
                            }
                        }
                    }

                    changes.push(DeltaChange::ComponentUpdated {
                        entity_id,
                        component_id: component_id.to_string(),
                        data: curr_component.data.clone(),
                    });
                }
            } else {
                changes.push(DeltaChange::ComponentAdded {
                    entity_id,
                    component_id: component_id.to_string(),
                    data: curr_component.data.clone(),
                });
            }
        }

        for component_id in prev_components.keys() {
            if !curr_components.contains_key(component_id) {
                changes.push(DeltaChange::ComponentRemoved {
                    entity_id,
                    component_id: component_id.to_string(),
                });
            }
        }
    }

    fn components_equal(&self, a: &SerializedComponent, b: &SerializedComponent) -> bool {
        if a.id != b.id {
            return false;
        }

        match (&a.data, &b.data) {
            (ComponentData::Binary(a_data), ComponentData::Binary(b_data)) => a_data == b_data,
            (ComponentData::Json(a_json), ComponentData::Json(b_json)) => a_json == b_json,
            (ComponentData::Structured(a_map), ComponentData::Structured(b_map)) => a_map == b_map,
            _ => false,
        }
    }

    pub fn reset(&mut self) {
        self.previous_snapshot = None;
    }

    pub fn get_previous_snapshot(&self) -> Option<&WorldSnapshot> {
        self.previous_snapshot.as_ref()
    }
}

impl Default for DeltaCompressor {
    fn default() -> Self {
        Self::new()
    }
}

pub struct FieldCompressor {
    enabled: bool,
}

impl FieldCompressor {
    pub fn new() -> Self {
        Self { enabled: true }
    }

    pub fn with_enabled(enabled: bool) -> Self {
        Self { enabled }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn compute_field_deltas(
        &self,
        prev: &SerializedComponent,
        curr: &SerializedComponent,
    ) -> Option<Vec<FieldDelta>> {
        if !self.enabled {
            return None;
        }

        match (&prev.data, &curr.data) {
            (ComponentData::Structured(prev_fields), ComponentData::Structured(curr_fields)) => {
                let mut deltas = Vec::new();

                for (field_id, curr_value) in curr_fields {
                    if let Some(prev_value) = prev_fields.get(field_id) {
                        if prev_value != curr_value {
                            deltas.push(FieldDelta {
                                field_id: field_id.clone(),
                                old_value: Some(prev_value.clone()),
                                new_value: curr_value.clone(),
                            });
                        }
                    } else {
                        deltas.push(FieldDelta {
                            field_id: field_id.clone(),
                            old_value: None,
                            new_value: curr_value.clone(),
                        });
                    }
                }

                for field_id in prev_fields.keys() {
                    if !curr_fields.contains_key(field_id) {
                        deltas.push(FieldDelta {
                            field_id: field_id.clone(),
                            old_value: prev_fields.get(field_id).cloned(),
                            new_value: FieldValue::Null,
                        });
                    }
                }

                Some(deltas)
            }
            (ComponentData::Json(prev_json_str), ComponentData::Json(curr_json_str)) => {
                if let (Ok(prev_json), Ok(curr_json)) = (
                    serde_json::from_str::<serde_json::Value>(prev_json_str),
                    serde_json::from_str::<serde_json::Value>(curr_json_str)
                ) {
                    if let (Some(prev_obj), Some(curr_obj)) = (prev_json.as_object(), curr_json.as_object()) {
                        let mut deltas = Vec::new();

                        for (key, curr_value) in curr_obj {
                            if let Some(prev_value) = prev_obj.get(key) {
                                if prev_value != curr_value {
                                    deltas.push(FieldDelta {
                                        field_id: key.clone(),
                                        old_value: Some(json_to_field_value(prev_value)),
                                        new_value: json_to_field_value(curr_value),
                                    });
                                }
                            } else {
                                deltas.push(FieldDelta {
                                    field_id: key.clone(),
                                    old_value: None,
                                    new_value: json_to_field_value(curr_value),
                                });
                            }
                        }

                        for key in prev_obj.keys() {
                            if !curr_obj.contains_key(key) {
                                deltas.push(FieldDelta {
                                    field_id: key.clone(),
                                    old_value: prev_obj.get(key).map(json_to_field_value),
                                    new_value: FieldValue::Null,
                                });
                            }
                        }

                        Some(deltas)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

impl Default for FieldCompressor {
    fn default() -> Self {
        Self::new()
    }
}

fn json_to_field_value(value: &serde_json::Value) -> FieldValue {
    match value {
        serde_json::Value::Null => FieldValue::Null,
        serde_json::Value::Bool(b) => FieldValue::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                FieldValue::I64(i)
            } else if let Some(u) = n.as_u64() {
                FieldValue::U64(u)
            } else if let Some(f) = n.as_f64() {
                FieldValue::F64(f)
            } else {
                FieldValue::Null
            }
        }
        serde_json::Value::String(s) => FieldValue::String(s.clone()),
        serde_json::Value::Array(arr) => {
            FieldValue::Array(arr.iter().map(json_to_field_value).collect())
        }
        serde_json::Value::Object(obj) => {
            let map = obj.iter()
                .map(|(k, v)| (k.clone(), json_to_field_value(v)))
                .collect();
            FieldValue::Map(map)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_delta_compression_initial() {
        let mut compressor = DeltaCompressor::new();

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
            timestamp: 100.0,
            version: "1.0.0".to_string(),
        };

        let delta = compressor.create_delta(snapshot);

        assert_eq!(delta.changes.len(), 2);
        assert!(matches!(delta.changes[0], DeltaChange::EntityAdded { .. }));
        assert!(matches!(delta.changes[1], DeltaChange::ComponentAdded { .. }));
    }

    #[test]
    fn test_delta_compression_update() {
        let mut compressor = DeltaCompressor::new();

        let snapshot1 = WorldSnapshot {
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
            timestamp: 100.0,
            version: "1.0.0".to_string(),
        };

        compressor.create_delta(snapshot1);

        let snapshot2 = WorldSnapshot {
            entities: vec![
                SerializedEntity {
                    id: 1,
                    components: vec![
                        SerializedComponent {
                            id: "Position".to_string(),
                            data: ComponentData::from_json_value(serde_json::json!({"x": 15.0, "y": 20.0})),
                        }
                    ],
                }
            ],
            timestamp: 200.0,
            version: "1.0.0".to_string(),
        };

        let delta = compressor.create_delta(snapshot2);

        assert!(delta.changes.iter().any(|c| matches!(c, DeltaChange::ComponentUpdated { .. } | DeltaChange::FieldsUpdated { .. })));
    }

    #[test]
    fn test_field_level_delta() {
        let compressor = FieldCompressor::new();

        let mut prev_fields = HashMap::new();
        prev_fields.insert("x".to_string(), FieldValue::F64(10.0));
        prev_fields.insert("y".to_string(), FieldValue::F64(20.0));

        let mut curr_fields = HashMap::new();
        curr_fields.insert("x".to_string(), FieldValue::F64(15.0));
        curr_fields.insert("y".to_string(), FieldValue::F64(20.0));

        let prev_component = SerializedComponent {
            id: "Position".to_string(),
            data: ComponentData::Structured(prev_fields),
        };

        let curr_component = SerializedComponent {
            id: "Position".to_string(),
            data: ComponentData::Structured(curr_fields),
        };

        let deltas = compressor.compute_field_deltas(&prev_component, &curr_component).unwrap();

        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].field_id, "x");
    }
}
