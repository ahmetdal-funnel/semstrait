//! Node metadata carried by every PlanNode

use crate::annotation::SemAnnotation;
use crate::schema::Schema;
use std::sync::Arc;
use uuid::Uuid;

/// Metadata attached to every PlanNode
#[derive(Debug, Clone)]
pub struct NodeMeta {
    /// Unique identifier for this node
    pub node_id: Uuid,
    /// Output schema (stable field ordinals).
    /// Wrapped in `Arc` so pass-through nodes (Filter, Sort, Fetch) share
    /// their parent's schema without deep-cloning.
    pub output_schema: Arc<Schema>,
    /// Semantic annotations
    pub annotations: Vec<SemAnnotation>,
}

impl NodeMeta {
    /// Create metadata with a new schema (wraps in Arc).
    /// Use for nodes that produce a new schema (Scan, Project, Aggregate, Join, Union).
    pub fn new(output_schema: Schema) -> Self {
        Self {
            node_id: Uuid::new_v4(),
            output_schema: Arc::new(output_schema),
            annotations: Vec::new(),
        }
    }

    /// Create metadata sharing an existing schema Arc.
    /// Use for pass-through nodes (Filter, Sort, Fetch) that don't alter the schema.
    pub fn new_shared(output_schema: Arc<Schema>) -> Self {
        Self {
            node_id: Uuid::new_v4(),
            output_schema,
            annotations: Vec::new(),
        }
    }

    pub fn with_annotations(mut self, annotations: Vec<SemAnnotation>) -> Self {
        self.annotations = annotations;
        self
    }

    pub fn add_annotation(&mut self, annotation: SemAnnotation) {
        self.annotations.push(annotation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Field;
    use semstrait_core::DataType;

    fn sample_schema() -> Schema {
        Schema::new(vec![
            Field::new("id", DataType::Integer),
            Field::new("name", DataType::String),
        ])
    }

    #[test]
    fn test_new_wraps_in_arc() {
        let meta = NodeMeta::new(sample_schema());
        assert_eq!(Arc::strong_count(&meta.output_schema), 1);
        assert_eq!(meta.output_schema.len(), 2);
    }

    #[test]
    fn test_new_shared_reuses_arc() {
        let schema = Arc::new(sample_schema());
        let meta = NodeMeta::new_shared(Arc::clone(&schema));
        assert!(Arc::ptr_eq(&meta.output_schema, &schema));
        assert_eq!(Arc::strong_count(&schema), 2);
    }

    #[test]
    fn test_filter_shares_parent_schema() {
        let parent = NodeMeta::new(sample_schema());
        let child = NodeMeta::new_shared(Arc::clone(&parent.output_schema));
        assert!(Arc::ptr_eq(&parent.output_schema, &child.output_schema));
    }

    #[test]
    fn test_project_creates_new_schema() {
        let parent = NodeMeta::new(sample_schema());
        let child = NodeMeta::new(Schema::new(vec![
            Field::new("computed", DataType::Number),
        ]));
        assert!(!Arc::ptr_eq(&parent.output_schema, &child.output_schema));
    }

    #[test]
    fn test_clone_increments_refcount() {
        let meta = NodeMeta::new(sample_schema());
        let cloned = meta.clone();
        assert!(Arc::ptr_eq(&meta.output_schema, &cloned.output_schema));
        assert_eq!(Arc::strong_count(&meta.output_schema), 2);
    }

    #[test]
    fn test_arc_deref_access() {
        let meta = NodeMeta::new(sample_schema());
        // Arc<Schema> auto-derefs to Schema — all field access works transparently
        assert_eq!(meta.output_schema.ordinal("id"), Some(0));
        assert_eq!(meta.output_schema.ordinal("name"), Some(1));
        assert_eq!(meta.output_schema.fields.len(), 2);
    }
}
