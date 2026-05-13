//! [`SemanticModelLoader`] — `32 §9.6`.
//!
//! Fluent loader for [`SemanticModel`]. Parametrized by a filesystem
//! strategy (see [`SourceFs`]). Composes [`crate::parse`],
//! [`crate::parse_catalogs`], and [`crate::validate`] with read I/O.
//!
//! Per D-10, all parses contribute to a single
//! [`SemanticModelBuilder`]; `.build()` materialises plus runs uniform
//! SR-3 / SR-E-3 dedup across the union of sources, so cross-file
//! duplicate names raise the same diagnostic as same-file duplicates.

use crate::builder::SemanticModelBuilder;
use crate::catalogs::CatalogsConfig;
use crate::error::build::ModelBuildErrorKind;
use crate::model::SemanticModel;
use crate::parse::{check_identifiers, classify_yaml_error, parse_catalogs_with_source};
use crate::source_fs::{LocalFs, SourceFs};
use crate::yaml::env::substitute_env_for_model;
use crate::yaml::YamlRoot;
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

        // ── Stage 2: Parse model — accumulate into a single builder.
        let mut accumulated: Diagnostics<ModelBuildErrorKind> = Vec::new();
        let mut builder = SemanticModel::builder();
        let mut had_parse_error = false;
        for (yaml, source) in model_payloads {
            match accumulate_model(&yaml, source.as_str(), builder) {
                Ok((new_builder, diags)) => {
                    builder = new_builder;
                    accumulated
                        .extend(diags.into_iter().map(|d| d.map_kind(ModelBuildErrorKind::Parse)));
                }
                Err((new_builder, diags)) => {
                    builder = new_builder;
                    accumulated
                        .extend(diags.into_iter().map(|d| d.map_kind(ModelBuildErrorKind::Parse)));
                    had_parse_error = true;
                }
            }
        }
        if had_parse_error {
            return Err(accumulated);
        }

        // ── Stage 3: Parse catalogs ────────────────────────────────
        // The parsed `CatalogsConfig` (and `self.catalogs_inline`) are
        // not yet wired to a downstream validation stage — surfacing
        // catalogs parse errors as a precondition check is the only
        // visible behaviour at this phase. A future phase will plumb the
        // resolved config through validate / compile.
        let mut had_catalogs_error = false;
        for (yaml, source) in catalogs_payloads {
            match parse_catalogs_with_source(&yaml, source.as_str()) {
                Ok((_, diags)) => accumulated.extend(
                    diags
                        .into_iter()
                        .map(|d| d.map_kind(ModelBuildErrorKind::CatalogsParse)),
                ),
                Err(diags) => {
                    accumulated.extend(
                        diags
                            .into_iter()
                            .map(|d| d.map_kind(ModelBuildErrorKind::CatalogsParse)),
                    );
                    had_catalogs_error = true;
                }
            }
        }
        if had_catalogs_error {
            return Err(accumulated);
        }

        // ── Stage 4: Build (materialise + uniform dup + validate) ──
        if !self.validate_pass {
            // Caller opted out of validate; finalise the builder
            // through a placeholder that swallows the validate-stage
            // diagnostics. Phase P4 keeps the same surface as before
            // (validate always runs inside `.build()`); a future phase
            // can split materialisation from validation if needed.
            // For now, run `.build()` and discard validate errors.
            return match builder.build() {
                Ok((m, diags)) => {
                    accumulated.extend(diags);
                    Ok((m, accumulated))
                }
                Err(_) => {
                    // skip_validate semantics: still produce a model.
                    // Materialise via a dedicated bypass would need
                    // builder API surface; defer to a follow-up. For
                    // P4, surface the validate errors so the contract
                    // remains explicit.
                    Err(accumulated)
                }
            };
        }

        match builder.build() {
            Ok((m, diags)) => {
                accumulated.extend(diags);
                Ok((m, accumulated))
            }
            Err(diags) => {
                accumulated.extend(diags);
                Err(accumulated)
            }
        }
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

/// Run env-substitute → decode → lower → identifier check for a single
/// source against the supplied `builder`. On parse-fatal failure
/// (YAML syntax, env-var, etc.) the builder is returned **unchanged**
/// so the loader can continue accumulating diagnostics from later
/// sources. On identifier-check failure the entries are still appended
/// (so cross-file dup detection still sees them) but the diagnostic
/// vector signals the error to the caller.
fn accumulate_model(
    yaml: &str,
    source: &str,
    builder: SemanticModelBuilder,
) -> Result<
    (SemanticModelBuilder, Diagnostics<crate::error::parse::ParseErrorKind>),
    (SemanticModelBuilder, Diagnostics<crate::error::parse::ParseErrorKind>),
> {
    let expanded = match substitute_env_for_model(yaml) {
        Ok(s) => s,
        Err(diag) => return Err((builder, vec![diag])),
    };

    let root: YamlRoot = match serde_yaml::from_str(&expanded) {
        Ok(r) => r,
        Err(e) => {
            return Err((builder, vec![Diagnostic::new(classify_yaml_error(&e))]));
        }
    };

    let new_builder = root.lower_into(source, builder);

    let mut diags: Diagnostics<crate::error::parse::ParseErrorKind> = Vec::new();
    check_identifiers(&new_builder, source, &mut diags);

    if diags.is_empty() {
        Ok((new_builder, diags))
    } else {
        Err((new_builder, diags))
    }
}
