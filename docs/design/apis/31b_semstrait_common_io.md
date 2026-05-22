---
prereqs: [00, 30, 31]
authoritative-for:
  - the `semstrait-common::io` module surface: the `Source` / `Sink` async traits, the `FromIoBytes` / `IntoIoBytes` conversion traits, the `Location` polymorphic enum, the `IoErrorKind` typed-kind enum
  - the back-end roster exposed under `semstrait-common::io::backends`: `memory`, `local`, `s3` (feature-gated)
  - the adoption of `object_store` (Apache Arrow) as the internal back-end implementation; `object_store` is NEVER part of the public surface
  - the async posture (tokio) and its consequences for targets (`--no-default-features` preserves the original zero-runtime-dep posture)
  - the feature-flag set: `io` (default ON) and `io-aws` (default OFF); the v1 back-end roster is frozen at `memory` / `local` / `s3`
  - the dependency posture that amends `31 §1.3` — core carries optional I/O runtime + back-end SDK deps, all gated
refined-by:
  - 32 §10.4 (`semstrait-model::io::{load_model, dump_model, load_catalogs, dump_catalogs}` — domain wrappers composing with `core::io`)
  - 33 §16.5 (`semstrait-manifest::io::{load_manifest, dump_manifest}` — manifest-level wrappers)
---

# 31b. `semstrait-common::io` — Byte-Blob I/O Transport Layer

`31b` pins the byte-blob I/O protocol that every `semstrait-*` crate layers its format-specific load/dump wrappers on top of. It extends `semstrait-common` with a single new module (`io`) whose surface is deliberately minimal: two async traits (`Source`, `Sink`) plus two conversion traits (`FromIoBytes`, `IntoIoBytes`), one polymorphic scheme-dispatching enum (`Location`), one typed-kind enum (`IoErrorKind`) implementing `Diagnose` per `30 §5.4`, and a small roster of back-end implementations.

Back-ends are thin wrappers over the `object_store` crate (Apache Arrow project): `object_store` provides the actual filesystem / S3 / in-memory machinery; `core::io` owns the public trait vocabulary, the `Location` scheme parser, and the `IoErrorKind` taxonomy. `object_store` is an implementation detail — consumers never see its types.

Domain-specific functions like `load_model` (`32 §10.4`), `load_catalogs` (`32 §10.4`), and `load_manifest` (`33 §16.5`) do **not** live here. They live in the crate that owns the corresponding typed artifact. Core owns the transport; consumers own the format.

## 1. Purpose and Scope

### 1.1 What `semstrait-common::io` OWNS

- The `Source` and `Sink` async traits that every back-end implements.
- The `FromIoBytes` and `IntoIoBytes` conversion traits that let `Source::read::<T>` and `Sink::write::<B>` accept a range of owned / borrowed types (`Bytes`, `Vec<u8>`, `String`, `&str`, `&[u8]`, …).
- The `Location` enum covering `Local`, `InMemory`, and (feature-gated) `S3` schemes, with `FromStr` parsing and `Source` / `Sink` implementations that dispatch internally.
- A process-global client cache for the `Location`-dispatch path so that repeated `s3://…` URIs share one `object_store` client per `(region, endpoint)` pair.
- The `IoErrorKind` typed-kind enum with its canonical variant set (`NotFound`, `PermissionDenied`, `Network`, `Unsupported`, `Malformed`) and its `Diagnose` impl per `31 §7.4`. Back-end calls return `Result<…, IoErrorKind>`; callers wrap into `Diagnostic<IoErrorKind>` at the call site (typically inside a domain wrapper that has source-level location).
- Back-end implementations: `backends::memory::InMemory`, `backends::local::LocalFile`, and (feature-gated) `backends::s3::S3Source` + `backends::s3::S3SourceBuilder`.
- The `io` feature flag (default ON) and the `io-aws` feature flag (default OFF).

### 1.2 What `semstrait-common::io` does NOT own

- `load_model` / `dump_model` / `load_catalogs` / `dump_catalogs`. Those live in `semstrait-model::io` (`32 §10.4`) because they reference `SemanticModel` / `CatalogsConfig`.
- `load_manifest` / `dump_manifest`. Those live in `semstrait-manifest::io` (`33 §16.5`) and reference `SemanticManifest`.
- Directory walking, `$include` directive handling, multi-file merging. **Out of scope forever** — callers that need multi-source aggregation perform their own enumeration (`std::fs::read_dir`, `object_store::ObjectStore::list`, or a CLI-level helper) and stitch the results themselves.
- Format decoding (YAML / JSON / MessagePack). Transport yields bytes; format is a domain-crate concern.
- Retry policies, CDN failover, credential rotation. `object_store` handles transient retries internally; higher-level policies are caller concerns.
- Conditional writes (compare-and-swap / if-none-match). v1 ships with last-writer-wins semantics; CAS can land later as a MINOR extension trait if a concrete need surfaces.

### 1.3 Design posture

Three invariants drive the shape:

1. **Cycle-free.** Core cannot import from `semstrait-model` / `semstrait-manifest` (they depend on core). Therefore core's `io` module knows about bytes, not about `SemanticModel` / `SemanticManifest`.
2. **Feature-gated cloud SDKs.** AWS (and future GCS / Azure / HTTP) back-ends compile only when opted-in, so library crates that only need local I/O don't pay the cloud-SDK compile cost.
3. **Polymorphic ergonomics via `Location`.** A consumer holding a `Location` value can call `src.read::<String>().await` without caring which back-end is underneath. `Location` is `Clone + Debug` and carried by value in diagnostics, caches, and audit logs.

## 2. Module Layout

Top-level `pub mod` structure of the new `io` module:

```
semstrait-common
├── expr, types, functions, constraints, diagnostic, error   (per 31 §2)
└── io                                                       ← NEW (feature "io", default ON)
    ├── Source            (trait)                            // §3
    ├── Sink              (trait)                            // §4
    ├── FromIoBytes       (trait)                            // §5
    ├── IntoIoBytes       (trait)                            // §5
    ├── Location          (enum, implements Source + Sink)   // §6
    ├── IoErrorKind       (enum, implements Diagnose)        // §7
    └── backends
        ├── memory::InMemory                                 // always under "io"
        ├── local::LocalFile                                 // always under "io"
        ├── s3::S3Source                                     // cfg(feature = "io-aws")
        └── s3::S3SourceBuilder                              // cfg(feature = "io-aws")
```

**Re-exports.** `semstrait_common::io::`* re-exports `Source`, `Sink`, `FromIoBytes`, `IntoIoBytes`, `Location`, `IoErrorKind`. Back-ends are reached through `semstrait_common::io::backends::{memory, local, s3}`; no back-end type is re-exported at the module root — the flat surface stays tiny, and back-end discovery goes through `backends::`.

**Placement rationale.** `io` is a top-level module beside `expr` / `types` / `functions` (`31 §2`). It is not nested under any existing module because it is cross-cutting: consumers of `expr` never need it, consumers of `types` never need it, but any crate that loads YAML from disk or from S3 does. A peer module keeps the discovery story flat and the feature-flag gating surgical.

---

## 3. The `Source` Trait

```rust
use bytes::Bytes;
use std::borrow::Cow;

/// A handle that can read a byte payload asynchronously.
///
/// All back-ends implement `Source`. Consumers that need polymorphic
/// back-end dispatch use `Location` (which itself implements `Source`).
///
/// Back-end methods return the bare `IoErrorKind`; callers wrap into a
/// `Diagnostic<IoErrorKind>` (per `30 §5.1`) at the call site if they
/// have caller-level location data to attach.
pub trait Source: Send + Sync {
    /// Read the full payload as raw bytes. This is the single method
    /// every back-end MUST implement; everything else is a default.
    fn read_raw(&self) -> impl Future<Output = Result<Bytes, IoErrorKind>> + Send;

    /// Read the full payload as any type that implements `FromIoBytes`.
    /// Typical call sites:
    ///   - `src.read::<Bytes>().await`    for zero-copy bytes
    ///   - `src.read::<Vec<u8>>().await`  for an owned vector
    ///   - `src.read::<String>().await`   for UTF-8 text (validates; emits `IoErrorKind::Malformed` on failure)
    ///
    /// Default impl delegates to `read_raw` + `T::from_io_bytes`.
    fn read<T: FromIoBytes>(&self) -> impl Future<Output = Result<T, IoErrorKind>> + Send
    where
        Self: Sync,
    {
        async move { T::from_io_bytes(self.read_raw().await?) }
    }

    /// Content-addressable stable identity of the source.
    ///
    /// Contract: if `a.describe() == b.describe()`, then `a.read_raw().await?`
    /// and `b.read_raw().await?` yield identical bytes (modulo concurrent
    /// mutation of the underlying store).
    ///
    /// Used by diagnostics, content-hashed caches, and audit logs. Safe to log
    /// at any level (no credentials, no authorization tokens).
    ///
    /// Examples: `"file:///abs/path/to/model.yaml"`, `"s3://bucket/key.yaml"`,
    /// `"mem:catalogs-fixture"`.
    fn describe(&self) -> Cow<'_, str>;
}
```

### 3.1 Async posture

`Source::read_raw` and `Source::read` use the stable async-fn-in-trait shape (Rust 1.75+) with an explicit `+ Send` bound on the returned future. Implementations that perform blocking work wrap via `tokio::task::spawn_blocking`. Consumers await inside their own runtime; `core::io` does not own the runtime and does not block.

### 3.2 Cancellation and timeouts

Standard tokio cancellation applies: dropping the future aborts the read mid-flight. No `CancellationToken` parameter is threaded through the trait. Callers enforce deadlines via `tokio::time::timeout(duration, src.read_raw())`. Implementations must not leave partial state visible on cancellation — in-memory back-ends never mutate on `read`; network back-ends drop the in-flight connection cleanly (delegated to `object_store`).

### 3.3 Idempotency

`read_raw` / `read` are idempotent from the caller's perspective: calling twice on the same `Source` returns the same payload unless the underlying store changed between calls.

### 3.4 Size limits

None at the transport level. A caller that loads a multi-gigabyte blob pays the memory cost. If a domain wrapper (e.g. `load_model`) wants to cap input size, it does so in its own layer. This matches `object_store`'s default posture.

### 3.5 `describe()` contract

`describe()` is the stable content-addressable identity of the source — NOT a human-readable free-form log message. The contract is:

> If two `Source` handles `a` and `b` satisfy `a.describe() == b.describe()`, then `a.read_raw()` and `b.read_raw()` yield identical bytes (absent concurrent mutation).

Consequences per back-end:

- `LocalFile::describe()` returns an absolute path string. Two `LocalFile` handles constructed from the same canonicalized path have equal `describe()`; a handle constructed from `./x.yaml` and one from `/abs/x.yaml` have *different* `describe()` even if the paths resolve to the same file. Canonicalization is the caller's responsibility if cache hits matter.
- `S3Source::describe()` returns `"s3://<bucket>/<key>"`. Trivially stable.
- `InMemory::describe()` returns `"mem:<name>"`, where `name` is supplied at construction (`InMemory::new(name, bytes)`). Anonymous in-memory sources are not supported — the contract requires a user-picked identity.
- `Location::describe()` delegates to the inner back-end.

---

## 4. The `Sink` Trait

```rust
use bytes::Bytes;
use std::borrow::Cow;

/// A handle that can write a byte payload asynchronously.
///
/// Writes are caller-atomic: either the full payload lands or the sink
/// leaves its previous state intact (no half-written artifacts).
///
/// Back-end methods return the bare `IoErrorKind`; callers wrap into a
/// `Diagnostic<IoErrorKind>` at the call site as for `Source`.
pub trait Sink: Send + Sync {
    /// Write raw bytes. The single method every back-end MUST implement.
    fn write_raw(&self, bytes: Bytes) -> impl Future<Output = Result<(), IoErrorKind>> + Send;

    /// Write any type that implements `IntoIoBytes` (accepts `Bytes`,
    /// `Vec<u8>`, `&[u8]`, `String`, `&str`, …).
    ///
    /// Default impl delegates to `write_raw` + `B::into_io_bytes`.
    fn write<B: IntoIoBytes + Send>(&self, data: B) -> impl Future<Output = Result<(), IoErrorKind>> + Send
    where
        Self: Sync,
    {
        async move { self.write_raw(data.into_io_bytes()).await }
    }

    /// Content-addressable stable identity (see `Source::describe()`).
    fn describe(&self) -> Cow<'_, str>;
}
```

### 4.1 Atomicity

- `LocalFile::write_raw` uses `object_store::local::LocalFileSystem::put`, which writes to a temp file and atomically renames to the target path. Readers either see the previous content or the new content — never a partial write.
- `S3Source::write_raw` uses a single `put_object` (or multipart upload for large payloads). S3's object-level strong consistency guarantees readers see either the prior key (if any) or the new one.
- `InMemory::write_raw` replaces the internal buffer under an internal lock.

### 4.2 Parent-directory creation

`LocalFile::write_raw` creates missing parent directories (`object_store` handles this). This matches the caller's intuition for "write to this path" and avoids boilerplate in every wrapper.

### 4.3 Concurrent writes

Last-writer-wins. Two tasks calling `sink.write_raw(bytes)` on the same underlying path / key produce whichever `write_raw` completes last — atomic per operation, but unordered across concurrent operations. No core-level coordination; no `Mutex` wrapping. Callers that need stronger guarantees serialize on their side (e.g. a `tokio::sync::Mutex` at the domain wrapper layer).

This matches `object_store`'s native semantics. Compare-and-swap / conditional writes are not ratified for v1; see §13.

---

## 5. `FromIoBytes` and `IntoIoBytes`

Centralize the byte-to-typed and typed-to-byte conversions so `Source::read<T>` and `Sink::write<B>` can accept a range of shapes without per-back-end code.

```rust
use bytes::Bytes;

/// Convert raw I/O bytes into a typed shape. Implementations MUST NOT
/// mutate the underlying store or perform I/O — this is pure conversion.
pub trait FromIoBytes: Sized {
    fn from_io_bytes(bytes: Bytes) -> Result<Self, IoErrorKind>;
}

impl FromIoBytes for Bytes {
    fn from_io_bytes(bytes: Bytes) -> Result<Self, IoErrorKind> { Ok(bytes) }
}

impl FromIoBytes for Vec<u8> {
    fn from_io_bytes(bytes: Bytes) -> Result<Self, IoErrorKind> { Ok(bytes.to_vec()) }
}

impl FromIoBytes for String {
    fn from_io_bytes(bytes: Bytes) -> Result<Self, IoErrorKind> {
        String::from_utf8(bytes.to_vec()).map_err(|e| IoErrorKind::Malformed {
            describe: String::from("<in-conversion>"),
            reason: format!("invalid UTF-8 at byte {}", e.utf8_error().valid_up_to()).into(),
        })
    }
}

/// Convert a typed value into raw I/O bytes for writing.
pub trait IntoIoBytes {
    fn into_io_bytes(self) -> Bytes;
}

impl IntoIoBytes for Bytes       { fn into_io_bytes(self) -> Bytes { self } }
impl IntoIoBytes for Vec<u8>     { fn into_io_bytes(self) -> Bytes { Bytes::from(self) } }
impl IntoIoBytes for &'_ [u8]    { fn into_io_bytes(self) -> Bytes { Bytes::copy_from_slice(self) } }
impl IntoIoBytes for String      { fn into_io_bytes(self) -> Bytes { Bytes::from(self) } }
impl IntoIoBytes for &'_ str     { fn into_io_bytes(self) -> Bytes { Bytes::copy_from_slice(self.as_bytes()) } }
```

Adding a new target type later (e.g. a zero-copy `Cow<'_, [u8]>`, a `serde`-adapter wrapper) is additive — just another `impl FromIoBytes for ...` or `impl IntoIoBytes for ...`. No trait-method addition on `Source` / `Sink` is needed.

---

## 6. `Location` — Polymorphic Back-End Dispatch

```rust
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum Location {
    Local(backends::local::LocalFile),
    InMemory(backends::memory::InMemory),
    #[cfg(feature = "io-aws")]
    S3(backends::s3::S3Source),
}
```

Both `Source` and `Sink` are implemented on `Location` by match-dispatching to the inner back-end's `read_raw` / `write_raw` / `describe`.

### 6.1 `FromStr` parsing

```rust
impl FromStr for Location {
    type Err = IoErrorKind;
    fn from_str(s: &str) -> Result<Self, IoErrorKind>;
}
```

Dispatch table:


| Input shape                       | Variant                                                                         | Notes                                                                                                              |
| --------------------------------- | ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `s3://<bucket>/<key>`             | `Location::S3(S3Source::new(bucket,key))`                                       | requires `io-aws`; raises `IoErrorKind::Unsupported` otherwise                                                     |
| `mem:<name>`                      | Looks up `<name>` in the in-memory registry, or returns `IoErrorKind::NotFound` | `mem:` URIs refer to `InMemory` instances registered via a process-global `InMemory::register(name, bytes)` helper |
| `file://<path>`                   | `Location::Local(LocalFile::new(<path>))`                                       | standard file-URI form                                                                                             |
| `/<abs>` / `./<rel>` / `../<rel>` | `Location::Local(LocalFile::new(input))`                                        | bare filesystem path                                                                                               |
| any other literal                 | `Location::Local(LocalFile::new(input))`                                        | default-to-local                                                                                                   |
| `<unknown-scheme>://<rest>`       | `IoErrorKind::Unsupported`                                                      | only known schemes dispatch; unknown schemes error                                                                 |


Parse errors raise `IoErrorKind::Malformed` for shape-violations (e.g. `s3://` with no bucket).

### 6.2 Client caching on the `Location::from_str` path

For S3, constructing a fresh `object_store::aws::AmazonS3` client on every `from_str` is expensive (default credential chain runs each time, and connection pooling is lost). The `from_str` path therefore consults a process-global cache keyed by `(region, endpoint_url)`:

```rust
static S3_CLIENT_CACHE: OnceLock<DashMap<ClientKey, Arc<object_store::aws::AmazonS3>>>
    = OnceLock::new();
```

- **Cache semantics.** First `from_str("s3://…")` for a given `(region, endpoint)` pair constructs the `AmazonS3` client and inserts it; subsequent calls reuse the cached client. Credentials are resolved inside the client (object_store manages refresh).
- **No eviction in v1.** Processes typically hit 1–3 distinct regions. An LRU cap lands as MINOR if ever needed.
- **Escape hatch.** Custom-built S3 clients (via `S3SourceBuilder`, §8) bypass the cache entirely; the builder produces a fresh `S3Source` that the caller stores and reuses.

### 6.4 Future extensions

`Location` is `#[non_exhaustive]`. Adding `Http { url, auth_opt }`, `Gcs { bucket, key }`, `Azure { container, blob }` is an additive MINOR per `30 §2.2`. Each new variant lands behind its own feature flag (`io-http`, `io-gcs`, `io-azure`). **None of these ship in v1** — the v1 roster is frozen at `Local` / `InMemory` / `S3`.

---

## 7. `IoErrorKind`

```rust
#[non_exhaustive]
#[derive(Debug)]
pub enum IoErrorKind {
    NotFound { describe: String },

    PermissionDenied { describe: String },

    Network {
        describe: String,
        reason: Cow<'static, str>,
        /// Foreign-error chain per `30 §5.4` (variant-side wrapping).
        /// The wrapped error participates in the std::error::Error chain
        /// via `Diagnose::cause()`.
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    Unsupported { describe: String, reason: Cow<'static, str> },

    Malformed { describe: String, reason: Cow<'static, str> },
}
```

**Variant taxonomy.**


| Variant            | Trigger                                                                                               | Example                                                                            |
| ------------------ | ----------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| `NotFound`         | Target path / key does not exist                                                                      | `LocalFile::read_raw` on a non-existent path; S3 `NoSuchKey`                       |
| `PermissionDenied` | Access denied by the back-end                                                                         | Unix `EACCES`; S3 `AccessDenied`                                                   |
| `Network`          | Transport-level failure (connection refused, DNS, timeout, TLS)                                       | S3 call timed out; endpoint unreachable                                            |
| `Unsupported`      | Scheme or back-end not available in the current feature set, or operation not supported by a back-end | `s3://` URL parsed without `io-aws` enabled; conditional write attempted           |
| `Malformed`        | Payload or URI violates the expected shape                                                            | UTF-8 decode failure via `String::from_io_bytes`; `s3://` with no bucket component |


### 7.1 `Diagnose` impl

```rust
impl Diagnose for IoErrorKind {
    fn message(&self) -> String {
        match self {
            Self::NotFound { describe } =>
                format!("not found: {describe}"),
            Self::PermissionDenied { describe } =>
                format!("permission denied: {describe}"),
            Self::Network { describe, reason, .. } =>
                format!("network error on {describe}: {reason}"),
            Self::Unsupported { describe, reason } =>
                format!("unsupported: {describe} ({reason})"),
            Self::Malformed { describe, reason } =>
                format!("malformed payload from {describe}: {reason}"),
        }
    }

    fn severity_default(&self) -> Severity { Severity::Error }

    fn cause(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Network { source: Some(e), .. } => Some(e.as_ref()),
            _ => None,
        }
    }
}

impl std::fmt::Display for IoErrorKind { /* delegates to Diagnose::message */ }
impl std::error::Error for IoErrorKind {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Diagnose::cause(self)
    }
}
```

### 7.2 Stage-of-origin and identification

Per `30 §5`, identification is by variant identity (renaming a variant is MAJOR; adding one inside `#[non_exhaustive]` is MINOR). There is no central code allocation; matching `IoErrorKind::NotFound { .. }` is the stable contract.

The stage of origin (`fs.read`, `repository.load`, etc.) appears as a `tracing` `stage` field on the surrounding span (`30 §6.5`), not in the kind. Domain wrappers building a `Diagnostic<IoErrorKind>` from a back-end call attach a caller-known location (file path, manifest ID) at construction.

### 7.3 Cross-crate composition

Domain error kinds in upstream crates (`semstrait_model::ModelLoadErrorKind`, `semstrait_manifest::RepositoryErrorKind`) declare `Io(IoErrorKind)` as a variant per D.ii cross-stage nesting (`30 §7.4`). Because `IoErrorKind` is `#[non_exhaustive]`, adding a variant here remains MINOR through the entire dependency chain.

Example (in `33 §11`):

```rust
#[non_exhaustive]
pub enum RepositoryErrorKind {
    Io(IoErrorKind),                    // wraps transport failure
    ManifestIdCollision { id: String }, // domain-level
    EncodingMismatch { expected: String, actual: String },
    IntegrityCheckFailed { reason: String },
}
```

---

## 8. Back-End Roster


| Back-end         | Module path                     | Implements       | Feature flag      | Internal wrapping                          |
| ---------------- | ------------------------------- | ---------------- | ----------------- | ------------------------------------------ |
| In-memory        | `backends::memory::InMemory`    | `Source`, `Sink` | always under `io` | `object_store::memory::InMemory`           |
| Local filesystem | `backends::local::LocalFile`    | `Source`, `Sink` | always under `io` | `object_store::local::LocalFileSystem`     |
| S3               | `backends::s3::S3Source`        | `Source`, `Sink` | `io-aws`          | `object_store::aws::AmazonS3`              |
| S3 builder       | `backends::s3::S3SourceBuilder` | (constructor)    | `io-aws`          | wraps `object_store::aws::AmazonS3Builder` |


### 8.1 `InMemory`

```rust
impl InMemory {
    /// Construct from a user-picked stable name and initial bytes.
    /// The name participates in `describe()` — two InMemory handles
    /// with the same name MUST carry identical bytes over their lifetime.
    pub fn new(name: impl Into<String>, bytes: impl IntoIoBytes) -> Self;

    /// Convenience: construct an empty sink registered under `name`.
    pub fn empty(name: impl Into<String>) -> Self;
}
```

`InMemory` is both `Source` and `Sink`. Writes replace the internal buffer under an internal lock. Intended for tests, inline fixtures, and deterministic replay pipelines.

### 8.2 `LocalFile`

```rust
impl LocalFile {
    pub fn new(path: impl Into<PathBuf>) -> Self;
}
```

`describe()` returns the path converted to an absolute form at call time (`std::path::absolute` or equivalent); no filesystem existence check. Caller controls canonicalization if `describe()` equality matters for caching.

### 8.3 `S3Source` and `S3SourceBuilder`

```rust
impl S3Source {
    /// Uses object_store's default credential chain and the Location-level
    /// client cache. Equivalent to Location::from_str("s3://bucket/key").
    pub fn new(bucket: impl Into<String>, key: impl Into<String>) -> Self;
}

impl S3SourceBuilder {
    pub fn new(bucket: impl Into<String>, key: impl Into<String>) -> Self;

    pub fn with_region(self, region: impl Into<String>)           -> Self;
    pub fn with_endpoint(self, url: impl Into<String>)            -> Self;
    pub fn with_credentials(self, access: impl Into<String>, secret: impl Into<String>) -> Self;
    pub fn with_session_token(self, token: impl Into<String>)     -> Self;
    pub fn with_allow_http(self, allow: bool)                     -> Self;

    /// Escape hatch — configure the underlying object_store builder directly.
    /// Advanced callers use this for retry policy, proxy config, custom HTTP
    /// client, and other options not mirrored in our thin surface.
    pub fn with_object_store_builder(
        self,
        builder: object_store::aws::AmazonS3Builder,
    ) -> Self;

    pub fn build(self) -> Result<S3Source, IoErrorKind>;
}
```

Clients built via `S3SourceBuilder` bypass the `Location`-level cache (the caller owns the client and is responsible for reuse).

### 8.4 Future roster

No new back-ends ship in v1. Adding `backends::http`, `backends::gcs`, `backends::azure` is a MINOR addition (see §6.4). All three are supported by `object_store` out of the box; the per-back-end code is ~30 LOC of trait delegation plus a feature flag.

---

## 9. Feature Flags


| Feature  | Default | Gates                                                                                                                          | Pulls                                                                                     |
| -------- | ------- | ------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------- |
| `io`     | **ON**  | The `io` module itself; `Source`, `Sink`, conversion traits, `Location`, `IoErrorKind`; `backends::memory` + `backends::local` | `tokio` (rt + fs), `bytes`, `object_store` (minimal — `Local` + `InMemory` features only) |
| `io-aws` | OFF     | `Location::S3` variant + `backends::s3::{S3Source, S3SourceBuilder}`                                                           | `object_store/aws` feature (pulls `aws-config` transitively)                              |


**No other I/O features in v1.** `io-gcs`, `io-azure`, `io-http`, `io-wasm` are not shipped; each would be an additive MINOR that enables the corresponding `object_store` feature.

### 9.1 Opting out entirely

```toml
[dependencies]
semstrait-common = { version = "…", default-features = false }
```

With `--no-default-features`, the `io` module disappears from `semstrait-common`. `tokio`, `bytes`, `object_store` are not in the dep graph. The crate's original "zero-runtime-dep leaf" posture (`31 §1.3`) is preserved. Consumers that only want `Expr` / `DataType` / `Diagnostic` — no I/O — take this path.

## 10. Dependency Posture


| Dep                                 | Gated by | Purpose                                                                                                     |
| ----------------------------------- | -------- | ----------------------------------------------------------------------------------------------------------- |
| `tokio`                             | `io`     | Async runtime + `tokio::fs` used internally by `object_store`; required for `async fn` in trait Send bounds |
| `bytes`                             | `io`     | `Bytes` zero-copy buffer type returned by `Source::read_raw` / accepted by `Sink::write_raw`                |
| `object_store`                      | `io`     | Back-end implementations (LocalFileSystem, InMemory — via minimal-features set)                             |
| `object_store` (with `aws` feature) | `io-aws` | S3 back-end (`AmazonS3` + builder)                                                                          |
| `thiserror`                         | —        | Already a `semstrait-common` dep for error types (`31 §12`)                                                   |
| `dashmap`                           | `io`     | `OnceLock<DashMap<ClientKey, Arc<AmazonS3>>>` for the `Location`-dispatch client cache                      |


**Amendment to `31 §1.3`.** The original "`semstrait-common` is the leaf of the workspace DAG — depends on nothing" posture is refined: under default features, `semstrait-common` depends on `tokio`, `bytes`, `object_store`, and `dashmap`. Under `--no-default-features`, the original zero-runtime-dep shape is preserved. This amendment is ratified here and cross-referenced in `31 §12`.

**No transitive dep escalation in downstream crates.** `semstrait-model` disables `io` in its default feature set and uses `parse(&str)`; it gains I/O by enabling its own `io` feature, which forwards to `semstrait-common/io`. `semstrait-api` / `semstrait-facade` / CLI enable `io-aws` explicitly if they need S3.

`**object_store` is not re-exported.** Consumers never see `object_store::ObjectStore`, `object_store::Path`, or any of its error types. The one exception is `S3SourceBuilder::with_object_store_builder`, which accepts an `object_store::aws::AmazonS3Builder` as the advanced escape hatch — callers who opt into this API implicitly opt into `object_store` evolution.

---

## 11. Platform Support


| Target                                            | `io`            | `io-aws`        | Notes                                                      |
| ------------------------------------------------- | --------------- | --------------- | ---------------------------------------------------------- |
| `x86_64-unknown-linux-gnu`, `aarch64-*-linux-gnu` | ✓               | ✓               | Primary target; full support                               |
| `x86_64-apple-darwin`, `aarch64-apple-darwin`     | ✓               | ✓               | Full support                                               |
| `x86_64-pc-windows-msvc`                          | ✓               | ✓               | Full support                                               |
| `wasm32-unknown-unknown`                          | **unsupported** | **unsupported** | **Out of scope in v1.** See §13 (closed).                  |
| `no_std` embedded                                 | disable `io`    | ✗               | `default-features = false` removes the I/O module entirely |


v1 does not support the wasm target. Adding wasm support (any flavour — `InMemory` only, `fetch`-backed HTTP, …) is out of scope for v1 and any follow-on MINOR unless a concrete tooling need surfaces.

---

## 12. Usage Patterns

### 12.1 Polymorphic via `Location` — one-liner

```rust
use std::str::FromStr;
use semstrait_common::io::{Location, Source};

let loc = Location::from_str("./model.yaml")?;
let text: String = loc.read().await?;
// caller hands `text` to semstrait_model::parse

// Or with an S3 URL (requires io-aws):
let loc = Location::from_str("s3://my-bucket/models/prod.yaml")?;
let bytes = loc.read_raw().await?;
let text: String = String::from_io_bytes(bytes)?;
```

### 12.2 Typed back-end directly

```rust
use semstrait_common::io::Source;
use semstrait_common::io::backends::local::LocalFile;

let src = LocalFile::new("./model.yaml");
let text: String = src.read().await?;       // generic read into String
let vec:  Vec<u8> = src.read().await?;      // or into an owned vec
```

### 12.3 Custom S3 configuration via the builder

```rust
use semstrait_common::io::backends::s3::S3SourceBuilder;

let src = S3SourceBuilder::new("my-bucket", "path/to/model.yaml")
    .with_region("eu-west-1")
    .with_endpoint("https://minio.internal:9000")    // MinIO / R2 / Wasabi / custom S3
    .with_credentials(access_key, secret_key)
    .with_allow_http(true)                            // for non-HTTPS dev endpoints
    .build()?;

let text: String = src.read().await?;
```

### 12.4 Polymorphic sink (dump)

```rust
use std::str::FromStr;
use semstrait_common::io::{Location, Sink};

let loc = Location::from_str("s3://my-bucket/artifacts/out.yaml")?;
let canonical: String = /* produced by semstrait_model::io::dump_model */;
loc.write(canonical).await?;            // &str / String / Vec<u8> / Bytes all accepted
```

### 12.5 Domain wrappers (typical caller path)

Consumers rarely call `Source::read_raw` / `Sink::write_raw` directly. They call the domain wrapper that combines transport with typed parse / serialize:

```rust
use semstrait_common::io::Location;
use semstrait_model::io::{load_model, dump_model, DumpMode};

let src = Location::from_str("./model.yaml")?;
let model = load_model(&src).await?;                      // read + parse

// ... model edits ...

let dst = Location::from_str("./model.canonical.yaml")?;
dump_model(&model, &dst, DumpMode::Canonical).await?;     // serialize + write
```

See `32 §10.4` for the `semstrait-model::io` wrappers and `33 §16.5` for the `semstrait-manifest::io` wrappers.

### 12.6 Tests — in-memory round-trip

```rust
use semstrait_common::io::backends::memory::InMemory;
use semstrait_model::io::load_model;

let src = InMemory::new(
    "orders-fixture",                                    // stable name for describe()
    "semantic_model:\n  datasets:\n    orders:\n      description: test\n",
);
let model = load_model(&src).await?;
assert!(model.find_public("orders").is_some());
```

---

## 13. Structural Rules (SR-IO-*)


| Rule         | Statement                                                                                                                                                                                                                                                                                      | Enforcement                                                                                           |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| **SR-IO-1**  | Every back-end implements at least `Source`. `Sink` is optional; read-only back-ends omit it.                                                                                                                                                                                                  | Per-back-end impl; compile-time.                                                                      |
| **SR-IO-2**  | `read_raw` returns `Bytes`. Typed reads go through `Source::read<T>` and `FromIoBytes`. UTF-8 validation lives in `FromIoBytes for String`, emitting `IoErrorKind::Malformed` on failure.                                                                                                      | Trait contract; the default-method impl ties the two together.                                        |
| **SR-IO-3**  | `describe()` is stable content-addressable identity (§3.5). Equal `describe()` ⇒ equal bytes (absent concurrent mutation). `describe()` MUST NOT emit secrets.                                                                                                                                 | Per-impl discipline; code review + unit test that `describe()` of identical back-end states is equal. |
| **SR-IO-4**  | `read_raw` / `read` are idempotent.                                                                                                                                                                                                                                                            | Trait contract.                                                                                       |
| **SR-IO-5**  | Error taxonomy is exhaustive for the v1 back-end set. New back-ends that cannot map to an existing `IoErrorKind` variant extend the enum per `30 §2.2` stability rules (`#[non_exhaustive]` makes additions MINOR).                                                                            | `#[non_exhaustive]` + additive-MINOR rules.                                                           |
| **SR-IO-6**  | Writes are per-operation atomic: mid-write crashes do not produce half-written artifacts. Concurrent writes are last-writer-wins (§4.3).                                                                                                                                                       | Per-back-end impl (delegated to `object_store`).                                                      |
| **SR-IO-7**  | `Location::from_str` is total over the input domain: every input produces either a valid `Location` or an `IoErrorKind`; no panics, no silent defaults beyond the explicit "default-to-local" fallback documented in §6.1.                                                                     | Parser contract.                                                                                      |
| **SR-IO-8**  | `object_store` is an internal detail. Consumers outside the `semstrait-`* workspace never see `object_store::ObjectStore`, `object_store::Path`, or any of its error types in a public signature. The one exception is `S3SourceBuilder::with_object_store_builder` (documented escape hatch). | Code review + API audit; enforced by visibility rules on the back-end modules.                        |
| **SR-IO-9**  | `InMemory::new` requires a stable `name` argument; anonymous in-memory back-ends are not supported. This preserves SR-IO-3.                                                                                                                                                                    | Constructor signature.                                                                                |
| **SR-IO-10** | Core's `io` module is feature-gated behind `io` (default ON) and `io-aws` (default OFF). `--no-default-features` restores the zero-runtime-dep posture of `31 §1.3`.                                                                                                                           | `Cargo.toml` feature definition + CI job that builds `--no-default-features`.                         |


---

*Cross-references in this document are by section (e.g. `31 §1.3`, `32 §10.4`). No code-path references are used, per `00 §8`.*