//! Per-data-kind builders — `32 §9.7.2`, `32 §9.7.3`.
//!
//! Each public type and its nested counterpart carries a single
//! `#[bon] impl T { #[builder] fn new(...) -> Self }` constructor. The
//! struct shape itself stays nested (`body.base.{name, extras}`); the
//! constructor projects the flat builder surface authors expect
//! (`T::builder(name).extras(...)...build()`). `Vec` children are
//! accumulated via `#[builder(field)]` + per-instance inserters written
//! in custom `impl<S: state_mod::State> TBuilder<S> { ... }` blocks per
//! `bon`'s typestate API. Public/Nested envelope split (R-7 / `26 §3`)
//! is enforced by structural absence — the Nested types simply do not
//! carry the `description` / `ai_context` / `semantic_interface`
//! triplet.
//!
//! Member ordering note: `bon` requires `#[builder(start_fn)]` →
//! `#[builder(field)]` → ordinary setters. The constructor argument
//! order below mirrors that contract rather than the underlying
//! struct's field order.

use crate::data_kind::base::{ComplexExtras, DataKindBase, LeafExtras};
use crate::data_kind::dataset::{Dataset, DatasetBody, NestedDataset};
use crate::data_kind::grainset::{Grainset, GrainsetBody, NestedGrainset};
use crate::data_kind::joinset::{Joinset, JoinsetBody, NestedJoinset};
use crate::data_kind::unionset::{NestedUnionset, UnionMode, Unionset, UnionsetBody};
use crate::entities::ai::AiContext;
use crate::entities::relationship::Relationship;
use crate::entities::semantic_interface::SemanticInterface;
use bon::bon;

// ── Dataset (Public) ────────────────────────────────────────────────

#[bon]
impl Dataset {
    #[builder(builder_type(name = DatasetBuilder, vis = "pub"), finish_fn = build)]
    pub fn new(
        #[builder(start_fn, into)] name: String,
        extras: Option<LeafExtras>,
        #[builder(into)] description: Option<String>,
        ai_context: Option<AiContext>,
        semantic_interface: Option<SemanticInterface>,
    ) -> Self {
        Dataset {
            body: DatasetBody {
                base: DataKindBase {
                    name,
                    extras: extras.unwrap_or_default(),
                },
            },
            description,
            ai_context,
            semantic_interface: semantic_interface.unwrap_or_default(),
        }
    }
}

// ── NestedDataset ───────────────────────────────────────────────────

#[bon]
impl NestedDataset {
    #[builder(builder_type(name = NestedDatasetBuilder, vis = "pub"), finish_fn = build)]
    pub fn new(
        #[builder(start_fn, into)] name: String,
        extras: Option<LeafExtras>,
    ) -> Self {
        NestedDataset {
            body: DatasetBody {
                base: DataKindBase {
                    name,
                    extras: extras.unwrap_or_default(),
                },
            },
        }
    }
}

// ── Grainset (Public) ───────────────────────────────────────────────

#[bon]
impl Grainset {
    #[builder(
        builder_type(name = GrainsetBuilder, vis = "pub"),
        state_mod(name = grainset_builder, vis = "pub(crate)"),
        finish_fn = build,
    )]
    pub fn new(
        #[builder(start_fn, into)] name: String,
        #[builder(field)] datasets: Vec<NestedDataset>,
        #[builder(field)] unionsets: Vec<NestedUnionset>,
        #[builder(field)] joinsets: Vec<NestedJoinset>,
        extras: Option<ComplexExtras>,
        #[builder(into)] description: Option<String>,
        ai_context: Option<AiContext>,
        semantic_interface: Option<SemanticInterface>,
    ) -> Self {
        Grainset {
            body: GrainsetBody {
                base: DataKindBase {
                    name,
                    extras: extras.unwrap_or_default(),
                },
                datasets,
                unionsets,
                joinsets,
            },
            description,
            ai_context,
            semantic_interface: semantic_interface.unwrap_or_default(),
        }
    }
}

impl<S: grainset_builder::State> GrainsetBuilder<S> {
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
}

// ── NestedGrainset ──────────────────────────────────────────────────

#[bon]
impl NestedGrainset {
    #[builder(
        builder_type(name = NestedGrainsetBuilder, vis = "pub"),
        state_mod(name = nested_grainset_builder, vis = "pub(crate)"),
        finish_fn = build,
    )]
    pub fn new(
        #[builder(start_fn, into)] name: String,
        #[builder(field)] datasets: Vec<NestedDataset>,
        #[builder(field)] unionsets: Vec<NestedUnionset>,
        #[builder(field)] joinsets: Vec<NestedJoinset>,
        extras: Option<ComplexExtras>,
    ) -> Self {
        NestedGrainset {
            body: GrainsetBody {
                base: DataKindBase {
                    name,
                    extras: extras.unwrap_or_default(),
                },
                datasets,
                unionsets,
                joinsets,
            },
        }
    }
}

impl<S: nested_grainset_builder::State> NestedGrainsetBuilder<S> {
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
}

// ── Unionset (Public) ───────────────────────────────────────────────

#[bon]
impl Unionset {
    #[builder(
        builder_type(name = UnionsetBuilder, vis = "pub"),
        state_mod(name = unionset_builder, vis = "pub(crate)"),
        finish_fn = build,
    )]
    pub fn new(
        #[builder(start_fn, into)] name: String,
        #[builder(field)] datasets: Vec<NestedDataset>,
        #[builder(field)] grainsets: Vec<NestedGrainset>,
        #[builder(field)] joinsets: Vec<NestedJoinset>,
        extras: Option<ComplexExtras>,
        #[builder(default)] mode: UnionMode,
        #[builder(into)] description: Option<String>,
        ai_context: Option<AiContext>,
        semantic_interface: Option<SemanticInterface>,
    ) -> Self {
        Unionset {
            body: UnionsetBody {
                base: DataKindBase {
                    name,
                    extras: extras.unwrap_or_default(),
                },
                mode,
                datasets,
                grainsets,
                joinsets,
            },
            description,
            ai_context,
            semantic_interface: semantic_interface.unwrap_or_default(),
        }
    }
}

impl<S: unionset_builder::State> UnionsetBuilder<S> {
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
}

// ── NestedUnionset ──────────────────────────────────────────────────

#[bon]
impl NestedUnionset {
    #[builder(
        builder_type(name = NestedUnionsetBuilder, vis = "pub"),
        state_mod(name = nested_unionset_builder, vis = "pub(crate)"),
        finish_fn = build,
    )]
    pub fn new(
        #[builder(start_fn, into)] name: String,
        #[builder(field)] datasets: Vec<NestedDataset>,
        #[builder(field)] grainsets: Vec<NestedGrainset>,
        #[builder(field)] joinsets: Vec<NestedJoinset>,
        extras: Option<ComplexExtras>,
        #[builder(default)] mode: UnionMode,
    ) -> Self {
        NestedUnionset {
            body: UnionsetBody {
                base: DataKindBase {
                    name,
                    extras: extras.unwrap_or_default(),
                },
                mode,
                datasets,
                grainsets,
                joinsets,
            },
        }
    }
}

impl<S: nested_unionset_builder::State> NestedUnionsetBuilder<S> {
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
}

// ── Joinset (Public) ────────────────────────────────────────────────

#[bon]
impl Joinset {
    #[builder(
        builder_type(name = JoinsetBuilder, vis = "pub"),
        state_mod(name = joinset_builder, vis = "pub(crate)"),
        finish_fn = build,
    )]
    pub fn new(
        #[builder(start_fn, into)] name: String,
        #[builder(field)] relationships: Vec<Relationship>,
        #[builder(field)] datasets: Vec<NestedDataset>,
        #[builder(field)] grainsets: Vec<NestedGrainset>,
        #[builder(field)] unionsets: Vec<NestedUnionset>,
        extras: Option<ComplexExtras>,
        #[builder(into)] description: Option<String>,
        ai_context: Option<AiContext>,
        semantic_interface: Option<SemanticInterface>,
    ) -> Self {
        Joinset {
            body: JoinsetBody {
                base: DataKindBase {
                    name,
                    extras: extras.unwrap_or_default(),
                },
                relationships,
                datasets,
                grainsets,
                unionsets,
            },
            description,
            ai_context,
            semantic_interface: semantic_interface.unwrap_or_default(),
        }
    }
}

impl<S: joinset_builder::State> JoinsetBuilder<S> {
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
}

// ── NestedJoinset ───────────────────────────────────────────────────

#[bon]
impl NestedJoinset {
    #[builder(
        builder_type(name = NestedJoinsetBuilder, vis = "pub"),
        state_mod(name = nested_joinset_builder, vis = "pub(crate)"),
        finish_fn = build,
    )]
    pub fn new(
        #[builder(start_fn, into)] name: String,
        #[builder(field)] relationships: Vec<Relationship>,
        #[builder(field)] datasets: Vec<NestedDataset>,
        #[builder(field)] grainsets: Vec<NestedGrainset>,
        #[builder(field)] unionsets: Vec<NestedUnionset>,
        extras: Option<ComplexExtras>,
    ) -> Self {
        NestedJoinset {
            body: JoinsetBody {
                base: DataKindBase {
                    name,
                    extras: extras.unwrap_or_default(),
                },
                relationships,
                datasets,
                grainsets,
                unionsets,
            },
        }
    }
}

impl<S: nested_joinset_builder::State> NestedJoinsetBuilder<S> {
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
}
