//! Per-data-kind hand-rolled builders — `32 §9.7.2`.
//!
//! The data-kind structs (`Dataset`, `Grainset`, `Unionset`, `Joinset`
//! and their `Nested*` siblings) have a nested storage shape
//! (`body: <Body> { base: DataKindBase { name, extras }, ... }`) but the
//! authoring surface is flat (callers write `.name(...)` and
//! `.extras(...)` directly on the data-kind builder). The builders in
//! this module flatten the `body.base.*` projection so authors operate
//! on a single fluent surface per `32 §9.7.2`.
//!
//! Method names equal Rust field names (R1). `Public` builders carry
//! the `description` / `ai_context` / `semantic_interface` triplet; the
//! `Nested*` builders deliberately omit those fields per `26 §3`
//! (R-7) — the type-level absence enforces the structural-only rule
//! without bespoke typestate machinery.

use crate::data_kind::base::{ComplexExtras, DataKindBase, LeafExtras};
use crate::data_kind::dataset::{Dataset, DatasetBody, NestedDataset};
use crate::data_kind::grainset::{Grainset, GrainsetBody, NestedGrainset};
use crate::data_kind::joinset::{Joinset, JoinsetBody, NestedJoinset};
use crate::data_kind::unionset::{NestedUnionset, UnionMode, Unionset, UnionsetBody};
use crate::entities::ai::AiContext;
use crate::entities::relationship::Relationship;
use crate::entities::semantic_interface::SemanticInterface;

// ── Dataset (Public) ────────────────────────────────────────────────

/// Fluent builder for [`Dataset`] — Public form.
#[derive(Debug, Clone)]
pub struct DatasetBuilder {
    name: String,
    extras: Option<LeafExtras>,
    description: Option<String>,
    ai_context: Option<AiContext>,
    semantic_interface: Option<SemanticInterface>,
}

impl Dataset {
    pub fn builder(name: impl Into<String>) -> DatasetBuilder {
        DatasetBuilder {
            name: name.into(),
            extras: None,
            description: None,
            ai_context: None,
            semantic_interface: None,
        }
    }
}

impl DatasetBuilder {
    pub fn extras(mut self, extras: LeafExtras) -> Self {
        self.extras = Some(extras);
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn ai_context(mut self, ai_context: AiContext) -> Self {
        self.ai_context = Some(ai_context);
        self
    }

    pub fn semantic_interface(mut self, iface: SemanticInterface) -> Self {
        self.semantic_interface = Some(iface);
        self
    }

    pub fn build(self) -> Dataset {
        Dataset {
            body: DatasetBody {
                base: DataKindBase {
                    name: self.name,
                    extras: self.extras.unwrap_or_default(),
                },
            },
            description: self.description,
            ai_context: self.ai_context,
            semantic_interface: self.semantic_interface.unwrap_or_default(),
        }
    }
}

// ── NestedDataset ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NestedDatasetBuilder {
    name: String,
    extras: Option<LeafExtras>,
}

impl NestedDataset {
    pub fn builder(name: impl Into<String>) -> NestedDatasetBuilder {
        NestedDatasetBuilder {
            name: name.into(),
            extras: None,
        }
    }
}

impl NestedDatasetBuilder {
    pub fn extras(mut self, extras: LeafExtras) -> Self {
        self.extras = Some(extras);
        self
    }

    pub fn build(self) -> NestedDataset {
        NestedDataset {
            body: DatasetBody {
                base: DataKindBase {
                    name: self.name,
                    extras: self.extras.unwrap_or_default(),
                },
            },
        }
    }
}

// ── Grainset (Public) ───────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct GrainsetBuilder {
    name: String,
    extras: Option<ComplexExtras>,
    datasets: Vec<NestedDataset>,
    unionsets: Vec<NestedUnionset>,
    joinsets: Vec<NestedJoinset>,
    description: Option<String>,
    ai_context: Option<AiContext>,
    semantic_interface: Option<SemanticInterface>,
}

impl Grainset {
    pub fn builder(name: impl Into<String>) -> GrainsetBuilder {
        GrainsetBuilder {
            name: name.into(),
            extras: None,
            datasets: Vec::new(),
            unionsets: Vec::new(),
            joinsets: Vec::new(),
            description: None,
            ai_context: None,
            semantic_interface: None,
        }
    }
}

impl GrainsetBuilder {
    pub fn extras(mut self, extras: ComplexExtras) -> Self {
        self.extras = Some(extras);
        self
    }

    pub fn dataset(mut self, child: NestedDataset) -> Self {
        self.datasets.push(child);
        self
    }

    pub fn datasets(mut self, children: impl IntoIterator<Item = NestedDataset>) -> Self {
        self.datasets.extend(children);
        self
    }

    pub fn unionset(mut self, child: NestedUnionset) -> Self {
        self.unionsets.push(child);
        self
    }

    pub fn unionsets(mut self, children: impl IntoIterator<Item = NestedUnionset>) -> Self {
        self.unionsets.extend(children);
        self
    }

    pub fn joinset(mut self, child: NestedJoinset) -> Self {
        self.joinsets.push(child);
        self
    }

    pub fn joinsets(mut self, children: impl IntoIterator<Item = NestedJoinset>) -> Self {
        self.joinsets.extend(children);
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn ai_context(mut self, ai_context: AiContext) -> Self {
        self.ai_context = Some(ai_context);
        self
    }

    pub fn semantic_interface(mut self, iface: SemanticInterface) -> Self {
        self.semantic_interface = Some(iface);
        self
    }

    pub fn build(self) -> Grainset {
        Grainset {
            body: GrainsetBody {
                base: DataKindBase {
                    name: self.name,
                    extras: self.extras.unwrap_or_default(),
                },
                datasets: self.datasets,
                unionsets: self.unionsets,
                joinsets: self.joinsets,
            },
            description: self.description,
            ai_context: self.ai_context,
            semantic_interface: self.semantic_interface.unwrap_or_default(),
        }
    }
}

// ── NestedGrainset ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NestedGrainsetBuilder {
    name: String,
    extras: Option<ComplexExtras>,
    datasets: Vec<NestedDataset>,
    unionsets: Vec<NestedUnionset>,
    joinsets: Vec<NestedJoinset>,
}

impl NestedGrainset {
    pub fn builder(name: impl Into<String>) -> NestedGrainsetBuilder {
        NestedGrainsetBuilder {
            name: name.into(),
            extras: None,
            datasets: Vec::new(),
            unionsets: Vec::new(),
            joinsets: Vec::new(),
        }
    }
}

impl NestedGrainsetBuilder {
    pub fn extras(mut self, extras: ComplexExtras) -> Self {
        self.extras = Some(extras);
        self
    }

    pub fn dataset(mut self, child: NestedDataset) -> Self {
        self.datasets.push(child);
        self
    }

    pub fn unionset(mut self, child: NestedUnionset) -> Self {
        self.unionsets.push(child);
        self
    }

    pub fn joinset(mut self, child: NestedJoinset) -> Self {
        self.joinsets.push(child);
        self
    }

    pub fn build(self) -> NestedGrainset {
        NestedGrainset {
            body: GrainsetBody {
                base: DataKindBase {
                    name: self.name,
                    extras: self.extras.unwrap_or_default(),
                },
                datasets: self.datasets,
                unionsets: self.unionsets,
                joinsets: self.joinsets,
            },
        }
    }
}

// ── Unionset (Public) ───────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct UnionsetBuilder {
    name: String,
    extras: Option<ComplexExtras>,
    mode: UnionMode,
    datasets: Vec<NestedDataset>,
    grainsets: Vec<NestedGrainset>,
    joinsets: Vec<NestedJoinset>,
    description: Option<String>,
    ai_context: Option<AiContext>,
    semantic_interface: Option<SemanticInterface>,
}

impl Unionset {
    pub fn builder(name: impl Into<String>) -> UnionsetBuilder {
        UnionsetBuilder {
            name: name.into(),
            extras: None,
            mode: UnionMode::default(),
            datasets: Vec::new(),
            grainsets: Vec::new(),
            joinsets: Vec::new(),
            description: None,
            ai_context: None,
            semantic_interface: None,
        }
    }
}

impl UnionsetBuilder {
    pub fn extras(mut self, extras: ComplexExtras) -> Self {
        self.extras = Some(extras);
        self
    }

    pub fn mode(mut self, mode: UnionMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn dataset(mut self, child: NestedDataset) -> Self {
        self.datasets.push(child);
        self
    }

    pub fn grainset(mut self, child: NestedGrainset) -> Self {
        self.grainsets.push(child);
        self
    }

    pub fn joinset(mut self, child: NestedJoinset) -> Self {
        self.joinsets.push(child);
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn ai_context(mut self, ai_context: AiContext) -> Self {
        self.ai_context = Some(ai_context);
        self
    }

    pub fn semantic_interface(mut self, iface: SemanticInterface) -> Self {
        self.semantic_interface = Some(iface);
        self
    }

    pub fn build(self) -> Unionset {
        Unionset {
            body: UnionsetBody {
                base: DataKindBase {
                    name: self.name,
                    extras: self.extras.unwrap_or_default(),
                },
                mode: self.mode,
                datasets: self.datasets,
                grainsets: self.grainsets,
                joinsets: self.joinsets,
            },
            description: self.description,
            ai_context: self.ai_context,
            semantic_interface: self.semantic_interface.unwrap_or_default(),
        }
    }
}

// ── NestedUnionset ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NestedUnionsetBuilder {
    name: String,
    extras: Option<ComplexExtras>,
    mode: UnionMode,
    datasets: Vec<NestedDataset>,
    grainsets: Vec<NestedGrainset>,
    joinsets: Vec<NestedJoinset>,
}

impl NestedUnionset {
    pub fn builder(name: impl Into<String>) -> NestedUnionsetBuilder {
        NestedUnionsetBuilder {
            name: name.into(),
            extras: None,
            mode: UnionMode::default(),
            datasets: Vec::new(),
            grainsets: Vec::new(),
            joinsets: Vec::new(),
        }
    }
}

impl NestedUnionsetBuilder {
    pub fn extras(mut self, extras: ComplexExtras) -> Self {
        self.extras = Some(extras);
        self
    }

    pub fn mode(mut self, mode: UnionMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn dataset(mut self, child: NestedDataset) -> Self {
        self.datasets.push(child);
        self
    }

    pub fn grainset(mut self, child: NestedGrainset) -> Self {
        self.grainsets.push(child);
        self
    }

    pub fn joinset(mut self, child: NestedJoinset) -> Self {
        self.joinsets.push(child);
        self
    }

    pub fn build(self) -> NestedUnionset {
        NestedUnionset {
            body: UnionsetBody {
                base: DataKindBase {
                    name: self.name,
                    extras: self.extras.unwrap_or_default(),
                },
                mode: self.mode,
                datasets: self.datasets,
                grainsets: self.grainsets,
                joinsets: self.joinsets,
            },
        }
    }
}

// ── Joinset (Public) ────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct JoinsetBuilder {
    name: String,
    extras: Option<ComplexExtras>,
    relationships: Vec<Relationship>,
    datasets: Vec<NestedDataset>,
    grainsets: Vec<NestedGrainset>,
    unionsets: Vec<NestedUnionset>,
    description: Option<String>,
    ai_context: Option<AiContext>,
    semantic_interface: Option<SemanticInterface>,
}

impl Joinset {
    pub fn builder(name: impl Into<String>) -> JoinsetBuilder {
        JoinsetBuilder {
            name: name.into(),
            extras: None,
            relationships: Vec::new(),
            datasets: Vec::new(),
            grainsets: Vec::new(),
            unionsets: Vec::new(),
            description: None,
            ai_context: None,
            semantic_interface: None,
        }
    }
}

impl JoinsetBuilder {
    pub fn extras(mut self, extras: ComplexExtras) -> Self {
        self.extras = Some(extras);
        self
    }

    pub fn relationship(mut self, r: Relationship) -> Self {
        self.relationships.push(r);
        self
    }

    pub fn relationships(mut self, items: impl IntoIterator<Item = Relationship>) -> Self {
        self.relationships.extend(items);
        self
    }

    pub fn dataset(mut self, child: NestedDataset) -> Self {
        self.datasets.push(child);
        self
    }

    pub fn grainset(mut self, child: NestedGrainset) -> Self {
        self.grainsets.push(child);
        self
    }

    pub fn unionset(mut self, child: NestedUnionset) -> Self {
        self.unionsets.push(child);
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn ai_context(mut self, ai_context: AiContext) -> Self {
        self.ai_context = Some(ai_context);
        self
    }

    pub fn semantic_interface(mut self, iface: SemanticInterface) -> Self {
        self.semantic_interface = Some(iface);
        self
    }

    pub fn build(self) -> Joinset {
        Joinset {
            body: JoinsetBody {
                base: DataKindBase {
                    name: self.name,
                    extras: self.extras.unwrap_or_default(),
                },
                relationships: self.relationships,
                datasets: self.datasets,
                grainsets: self.grainsets,
                unionsets: self.unionsets,
            },
            description: self.description,
            ai_context: self.ai_context,
            semantic_interface: self.semantic_interface.unwrap_or_default(),
        }
    }
}

// ── NestedJoinset ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NestedJoinsetBuilder {
    name: String,
    extras: Option<ComplexExtras>,
    relationships: Vec<Relationship>,
    datasets: Vec<NestedDataset>,
    grainsets: Vec<NestedGrainset>,
    unionsets: Vec<NestedUnionset>,
}

impl NestedJoinset {
    pub fn builder(name: impl Into<String>) -> NestedJoinsetBuilder {
        NestedJoinsetBuilder {
            name: name.into(),
            extras: None,
            relationships: Vec::new(),
            datasets: Vec::new(),
            grainsets: Vec::new(),
            unionsets: Vec::new(),
        }
    }
}

impl NestedJoinsetBuilder {
    pub fn extras(mut self, extras: ComplexExtras) -> Self {
        self.extras = Some(extras);
        self
    }

    pub fn relationship(mut self, r: Relationship) -> Self {
        self.relationships.push(r);
        self
    }

    pub fn dataset(mut self, child: NestedDataset) -> Self {
        self.datasets.push(child);
        self
    }

    pub fn grainset(mut self, child: NestedGrainset) -> Self {
        self.grainsets.push(child);
        self
    }

    pub fn unionset(mut self, child: NestedUnionset) -> Self {
        self.unionsets.push(child);
        self
    }

    pub fn build(self) -> NestedJoinset {
        NestedJoinset {
            body: JoinsetBody {
                base: DataKindBase {
                    name: self.name,
                    extras: self.extras.unwrap_or_default(),
                },
                relationships: self.relationships,
                datasets: self.datasets,
                grainsets: self.grainsets,
                unionsets: self.unionsets,
            },
        }
    }
}
