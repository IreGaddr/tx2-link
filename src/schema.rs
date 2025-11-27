use crate::error::{LinkError, Result};
use crate::protocol::{ComponentId, FieldId, FieldType};
use ahash::AHashMap;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

pub type SchemaVersion = u32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentSchema {
    pub component_id: ComponentId,
    pub version: SchemaVersion,
    pub fields: Vec<FieldSchema>,
    pub description: Option<String>,
}

impl ComponentSchema {
    pub fn new(component_id: ComponentId, version: SchemaVersion) -> Self {
        Self {
            component_id,
            version,
            fields: Vec::new(),
            description: None,
        }
    }

    pub fn with_field(mut self, field: FieldSchema) -> Self {
        self.fields.push(field);
        self
    }

    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    pub fn get_field(&self, field_id: &str) -> Option<&FieldSchema> {
        self.fields.iter().find(|f| f.field_id == field_id)
    }

    pub fn validate_field(&self, field_id: &str, field_type: &FieldType) -> bool {
        if let Some(schema) = self.get_field(field_id) {
            &schema.field_type == field_type
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSchema {
    pub field_id: FieldId,
    pub field_type: FieldType,
    pub optional: bool,
    pub default_value: Option<String>,
    pub description: Option<String>,
}

impl FieldSchema {
    pub fn new(field_id: FieldId, field_type: FieldType) -> Self {
        Self {
            field_id,
            field_type,
            optional: false,
            default_value: None,
            description: None,
        }
    }

    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }

    pub fn with_default(mut self, default: String) -> Self {
        self.default_value = Some(default);
        self
    }

    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }
}

pub struct SchemaRegistry {
    schemas: Arc<RwLock<AHashMap<ComponentId, ComponentSchema>>>,
    version_history: Arc<RwLock<AHashMap<ComponentId, Vec<SchemaVersion>>>>,
    current_version: SchemaVersion,
}

impl SchemaRegistry {
    pub fn new() -> Self {
        Self {
            schemas: Arc::new(RwLock::new(AHashMap::new())),
            version_history: Arc::new(RwLock::new(AHashMap::new())),
            current_version: 1,
        }
    }

    pub fn register(&self, schema: ComponentSchema) -> Result<()> {
        let mut schemas = self.schemas.write()
            .map_err(|e| LinkError::Unknown(format!("Lock poisoned: {}", e)))?;

        let mut version_history = self.version_history.write()
            .map_err(|e| LinkError::Unknown(format!("Lock poisoned: {}", e)))?;

        let component_id = schema.component_id.clone();
        let version = schema.version;

        if let Some(existing) = schemas.get(&component_id) {
            if existing.version >= version {
                return Err(LinkError::Unknown(
                    format!("Schema version {} already exists or is newer for component {}", version, component_id)
                ));
            }
        }

        version_history.entry(component_id.clone())
            .or_insert_with(Vec::new)
            .push(version);

        schemas.insert(component_id, schema);

        Ok(())
    }

    pub fn get(&self, component_id: &str) -> Result<ComponentSchema> {
        let schemas = self.schemas.read()
            .map_err(|e| LinkError::Unknown(format!("Lock poisoned: {}", e)))?;

        schemas.get(component_id)
            .cloned()
            .ok_or_else(|| LinkError::SchemaNotFound(component_id.to_string()))
    }

    pub fn get_version(&self, component_id: &str, version: SchemaVersion) -> Result<ComponentSchema> {
        let schema = self.get(component_id)?;

        if schema.version == version {
            Ok(schema)
        } else {
            Err(LinkError::SchemaMismatch {
                expected: version.to_string(),
                actual: schema.version.to_string(),
            })
        }
    }

    pub fn has(&self, component_id: &str) -> bool {
        self.schemas.read()
            .map(|schemas| schemas.contains_key(component_id))
            .unwrap_or(false)
    }

    pub fn get_all(&self) -> Result<Vec<ComponentSchema>> {
        let schemas = self.schemas.read()
            .map_err(|e| LinkError::Unknown(format!("Lock poisoned: {}", e)))?;

        Ok(schemas.values().cloned().collect())
    }

    pub fn get_version_history(&self, component_id: &str) -> Result<Vec<SchemaVersion>> {
        let history = self.version_history.read()
            .map_err(|e| LinkError::Unknown(format!("Lock poisoned: {}", e)))?;

        Ok(history.get(component_id)
            .cloned()
            .unwrap_or_default())
    }

    pub fn validate_compatibility(&self, old_version: SchemaVersion, new_version: SchemaVersion) -> bool {
        new_version >= old_version
    }

    pub fn clear(&self) -> Result<()> {
        let mut schemas = self.schemas.write()
            .map_err(|e| LinkError::Unknown(format!("Lock poisoned: {}", e)))?;

        let mut version_history = self.version_history.write()
            .map_err(|e| LinkError::Unknown(format!("Lock poisoned: {}", e)))?;

        schemas.clear();
        version_history.clear();

        Ok(())
    }

    pub fn get_current_version(&self) -> SchemaVersion {
        self.current_version
    }

    pub fn set_current_version(&mut self, version: SchemaVersion) {
        self.current_version = version;
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for SchemaRegistry {
    fn clone(&self) -> Self {
        Self {
            schemas: Arc::clone(&self.schemas),
            version_history: Arc::clone(&self.version_history),
            current_version: self.current_version,
        }
    }
}

pub struct SchemaValidator {
    registry: SchemaRegistry,
}

impl SchemaValidator {
    pub fn new(registry: SchemaRegistry) -> Self {
        Self { registry }
    }

    pub fn validate_component(&self, component_id: &str, fields: &AHashMap<FieldId, FieldType>) -> Result<()> {
        let schema = self.registry.get(component_id)?;

        for field_schema in &schema.fields {
            if !field_schema.optional {
                if !fields.contains_key(&field_schema.field_id) {
                    return Err(LinkError::InvalidMessage(
                        format!("Required field '{}' missing in component '{}'", field_schema.field_id, component_id)
                    ));
                }
            }

            if let Some(field_type) = fields.get(&field_schema.field_id) {
                if field_type != &field_schema.field_type {
                    return Err(LinkError::InvalidMessage(
                        format!("Field '{}' has wrong type in component '{}'", field_schema.field_id, component_id)
                    ));
                }
            }
        }

        Ok(())
    }

    pub fn get_registry(&self) -> &SchemaRegistry {
        &self.registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_registry() {
        let registry = SchemaRegistry::new();

        let schema = ComponentSchema::new("Position".to_string(), 1)
            .with_field(FieldSchema::new("x".to_string(), FieldType::F64))
            .with_field(FieldSchema::new("y".to_string(), FieldType::F64))
            .with_description("2D position component".to_string());

        registry.register(schema.clone()).unwrap();

        let retrieved = registry.get("Position").unwrap();
        assert_eq!(retrieved.component_id, "Position");
        assert_eq!(retrieved.fields.len(), 2);
    }

    #[test]
    fn test_schema_versioning() {
        let registry = SchemaRegistry::new();

        let schema_v1 = ComponentSchema::new("Position".to_string(), 1)
            .with_field(FieldSchema::new("x".to_string(), FieldType::F64))
            .with_field(FieldSchema::new("y".to_string(), FieldType::F64));

        registry.register(schema_v1).unwrap();

        let schema_v2 = ComponentSchema::new("Position".to_string(), 2)
            .with_field(FieldSchema::new("x".to_string(), FieldType::F64))
            .with_field(FieldSchema::new("y".to_string(), FieldType::F64))
            .with_field(FieldSchema::new("z".to_string(), FieldType::F64).optional());

        registry.register(schema_v2).unwrap();

        let history = registry.get_version_history("Position").unwrap();
        assert_eq!(history.len(), 2);
        assert!(history.contains(&1));
        assert!(history.contains(&2));
    }

    #[test]
    fn test_schema_validation() {
        let registry = SchemaRegistry::new();

        let schema = ComponentSchema::new("Position".to_string(), 1)
            .with_field(FieldSchema::new("x".to_string(), FieldType::F64))
            .with_field(FieldSchema::new("y".to_string(), FieldType::F64));

        registry.register(schema).unwrap();

        let validator = SchemaValidator::new(registry);

        let mut fields = AHashMap::new();
        fields.insert("x".to_string(), FieldType::F64);
        fields.insert("y".to_string(), FieldType::F64);

        assert!(validator.validate_component("Position", &fields).is_ok());

        let mut invalid_fields = AHashMap::new();
        invalid_fields.insert("x".to_string(), FieldType::F64);

        assert!(validator.validate_component("Position", &invalid_fields).is_err());
    }
}
