/// Load a test fixture YAML model by name (without extension).
///
/// Reads from `tests/fixtures/models/{name}.yaml` relative to the workspace root.
pub fn load_model(name: &str) -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let path = format!("{}/tests/fixtures/models/{}.yaml", manifest_dir, name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to load fixture '{}': {}", path, e))
}
