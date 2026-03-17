//! Node metadata carried by every PlanNode

use crate::annotation::SemAnnotation;
use crate::schema::Schema;
use uuid::Uuid;

/// Metadata attached to every PlanNode
#[derive(Debug, Clone)]
pub struct NodeMeta {
    /// Unique identifier for this node
    pub node_id: Uuid,
    /// Output schema (stable field ordinals)
    pub output_schema: Schema,
    /// Semantic annotations
    pub annotations: Vec<SemAnnotation>,
}

impl NodeMeta {
    pub fn new(output_schema: Schema) -> Self {
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
