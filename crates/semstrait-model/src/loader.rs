//! [`SemanticModelLoader`] — `32 §9.6`.
//!
//! Fluent loader for [`SemanticModel`]. Parametrized by a filesystem
//! strategy (see [`SourceFs`]). Composes [`crate::parse`],
//! [`crate::parse_catalogs`], and [`crate::validate`] with read I/O.

use crate::catalogs::CatalogsConfig;
use crate::error::build::ModelBuildErrorKind;
use crate::error::catalogs::CatalogsParseErrorKind;
use crate::error::parse::ParseErrorKind;
use crate::error::validate::ValidateErrorKind;
use crate::model::SemanticModel;
use crate::parse::{parse_catalogs_with_source, parse_with_source};
use crate::source_fs::{LocalFs, SourceFs};
use crate::validate::validate;
use semstrait_core::diagnostic::{Diagnostic, Diagnostics, SourceId};

/// Configured input — either inlined YAML or a path to read.
#[derive(Debug, Clone)]
enum LoadInput {
    Inline { yaml: String, source: SourceId },
    File { path: String, source: SourceId },
}

/// Fluent loader. Default strategy is [`LocalFs`]; switch via
/// [`SemanticModelLoader::with_fs`].
#[derive(Debug, Clone)]
pub struct SemanticModelLoader<F: SourceFs = LocalFs> {
    fs: F,
    model_inputs: Vec<LoadInput>,
    catalogs_inputs: Vec<LoadInput>,
    catalogs_inline: Option<CatalogsConfig>,
    validate_pass: bool,
}

impl Default for SemanticModelLoader<LocalFs> {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticModelLoader<LocalFs> {
    /// Default-strategy entry. Equivalent to
    /// [`SemanticModel::loader`].
    pub fn new() -> Self {
        Self {
            fs: LocalFs,
            model_inputs: Vec::new(),
            catalogs_inputs: Vec::new(),
            catalogs_inline: None,
            validate_pass: true,
        }
    }
}

impl SemanticModel {
    /// Convenience entry — equivalent to
    /// [`SemanticModelLoader::<LocalFs>::new`].
    pub fn loader() -> SemanticModelLoader<LocalFs> {
        SemanticModelLoader::new()
    }
}

impl<F: SourceFs> SemanticModelLoader<F> {
    /// Swap the filesystem strategy.
    pub fn with_fs<F2: SourceFs>(self, fs: F2) -> SemanticModelLoader<F2> {
        SemanticModelLoader {
            fs,
            model_inputs: self.model_inputs,
            catalogs_inputs: self.catalogs_inputs,
            catalogs_inline: self.catalogs_inline,
            validate_pass: self.validate_pass,
        }
    }

    /// Attach an in-memory YAML payload tagged with a logical
    /// [`SourceId`]. Multiple calls accumulate.
    pub fn from_yaml_str(mut self, yaml: impl Into<String>, source: SourceId) -> Self {
        self.model_inputs.push(LoadInput::Inline {
            yaml: yaml.into(),
            source,
        });
        self
    }

    /// Resolve `path` via the configured [`SourceFs`] at
    /// [`Self::build`] time. The logical [`SourceId`] is set to `path`.
    pub fn from_yaml_file(mut self, path: impl Into<String>) -> Self {
        let path = path.into();
        let source = SourceId(path.clone());
        self.model_inputs.push(LoadInput::File { path, source });
        self
    }

    /// Attach a pre-parsed [`CatalogsConfig`]. Replaces any prior
    /// catalogs configuration (last-write-wins).
    pub fn with_catalogs(mut self, c: CatalogsConfig) -> Self {
        self.catalogs_inline = Some(c);
        self.catalogs_inputs.clear();
        self
    }

    /// Attach an in-memory `catalogs.yaml` payload.
    pub fn from_catalogs_yaml_str(mut self, yaml: impl Into<String>, source: SourceId) -> Self {
        self.catalogs_inputs.push(LoadInput::Inline {
            yaml: yaml.into(),
            source,
        });
        self
    }

    /// Resolve a `catalogs.yaml` path via the configured [`SourceFs`].
    pub fn from_catalogs_yaml_file(mut self, path: impl Into<String>) -> Self {
        let path = path.into();
        let source = SourceId(path.clone());
        self.catalogs_inputs.push(LoadInput::File { path, source });
        self
    }

    /// Skip the validate pass. Default: validate runs.
    pub fn skip_validate(mut self) -> Self {
        self.validate_pass = false;
        self
    }

    /// Run the configured pipeline.
    pub fn build(
        self,
    ) -> Result<(SemanticModel, Diagnostics<ModelBuildErrorKind>), Diagnostics<ModelBuildErrorKind>>
    {
        if self.model_inputs.is_empty() {
            let d = Diagnostic::new(ModelBuildErrorKind::NoSource);
            return Err(vec![d]);
        }

        // ── Stage 1: Read ──────────────────────────────────────────
        let model_payloads = self.read_inputs(&self.model_inputs)?;
        let catalogs_payloads = self.read_inputs(&self.catalogs_inputs)?;

        // ── Stage 2: Parse model ───────────────────────────────────
        let mut accumulated: Diagnostics<ModelBuildErrorKind> = Vec::new();
        let mut model = SemanticModel::default();
        let mut had_parse_error = false;
        for (yaml, source) in model_payloads {
            match parse_with_source(&yaml, source.as_str()) {
                Ok((m, diags)) => {
                    accumulated.extend(diags.into_iter().map(lift_parse));
                    model = merge_models(model, m);
                }
                Err(diags) => {
                    accumulated.extend(diags.into_iter().map(lift_parse));
                    had_parse_error = true;
                }
            }
        }
        if had_parse_error {
            return Err(accumulated);
        }

        // ── Stage 3: Parse catalogs ────────────────────────────────
        let mut catalogs_loaded = self.catalogs_inline.clone();
        let mut had_catalogs_error = false;
        for (yaml, source) in catalogs_payloads {
            match parse_catalogs_with_source(&yaml, source.as_str()) {
                Ok((c, diags)) => {
                    accumulated.extend(diags.into_iter().map(lift_catalogs_parse));
                    catalogs_loaded = Some(c);
                }
                Err(diags) => {
                    accumulated.extend(diags.into_iter().map(lift_catalogs_parse));
                    had_catalogs_error = true;
                }
            }
        }
        let _ = catalogs_loaded;
        if had_catalogs_error {
            return Err(accumulated);
        }

        // ── Stage 4: Validate ──────────────────────────────────────
        if self.validate_pass {
            match validate(&model) {
                Ok(diags) => {
                    accumulated.extend(diags.into_iter().map(lift_validate));
                }
                Err(diags) => {
                    accumulated.extend(diags.into_iter().map(lift_validate));
                    return Err(accumulated);
                }
            }
        }

        Ok((model, accumulated))
    }

    fn read_inputs(
        &self,
        inputs: &[LoadInput],
    ) -> Result<Vec<(String, SourceId)>, Diagnostics<ModelBuildErrorKind>> {
        let mut out = Vec::new();
        let mut errors = Vec::new();
        for input in inputs {
            match input {
                LoadInput::Inline { yaml, source } => {
                    out.push((yaml.clone(), source.clone()));
                }
                LoadInput::File { path, source } => match self.fs.read(path) {
                    Ok(bytes) => match String::from_utf8(bytes) {
                        Ok(s) => out.push((s, source.clone())),
                        Err(_) => {
                            errors.push(Diagnostic::new(ModelBuildErrorKind::SourceIo {
                                path: path.clone(),
                                error: std::io::ErrorKind::InvalidData,
                            }));
                        }
                    },
                    Err(e) => {
                        errors.push(Diagnostic::new(ModelBuildErrorKind::SourceIo {
                            path: path.clone(),
                            error: e.kind(),
                        }));
                    }
                },
            }
        }
        if errors.is_empty() {
            Ok(out)
        } else {
            Err(errors)
        }
    }
}

fn merge_models(mut acc: SemanticModel, m: SemanticModel) -> SemanticModel {
    if acc.name.is_empty() {
        acc.name = m.name;
    }
    acc.description = acc.description.or(m.description);
    acc.ai_context = acc.ai_context.or(m.ai_context);
    acc.labels.extend(m.labels);
    acc.datasets.extend(m.datasets);
    acc.grainsets.extend(m.grainsets);
    acc.unionsets.extend(m.unionsets);
    acc.joinsets.extend(m.joinsets);
    acc.dimensions.extend(m.dimensions);
    acc.measures.extend(m.measures);
    acc.metrics.extend(m.metrics);
    acc.relationships.extend(m.relationships);
    acc
}

fn lift_parse(d: Diagnostic<ParseErrorKind>) -> Diagnostic<ModelBuildErrorKind> {
    Diagnostic {
        kind: ModelBuildErrorKind::Parse(d.kind),
        severity: d.severity,
        location: d.location,
        notes: d.notes,
    }
}

fn lift_catalogs_parse(
    d: Diagnostic<CatalogsParseErrorKind>,
) -> Diagnostic<ModelBuildErrorKind> {
    Diagnostic {
        kind: ModelBuildErrorKind::CatalogsParse(d.kind),
        severity: d.severity,
        location: d.location,
        notes: d.notes,
    }
}

fn lift_validate(d: Diagnostic<ValidateErrorKind>) -> Diagnostic<ModelBuildErrorKind> {
    Diagnostic {
        kind: ModelBuildErrorKind::Validate(d.kind),
        severity: d.severity,
        location: d.location,
        notes: d.notes,
    }
}

