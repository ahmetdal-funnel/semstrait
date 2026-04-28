---
prereqs: [00, 10, 15, 30, 31, 31b, 32]
authoritative-for:
  - the `catalogs.yaml` file shape: root key (`catalogs:`), per-entry alias, `type`, `name`, `url`, `realm`, `default_namespace`, `auth`
  - the `CatalogEntry` / `CatalogAuthMethod` typed roster (Oauth2 / Bearer / AwsSecrets variants)
  - the `CatalogRef` reference shape authored inside a model's `extras.catalog:` — a bare alias string newtype (no reference-site namespace override)
  - the `parse_catalogs` loader contract — pure, synchronous, no I/O beyond `std::env::var`
  - the shared-with-model async wrapper surface (`load_catalogs` / `dump_catalogs`) that lives in `semstrait-model::io` per `32 §10.4`
  - deterministic-ordering guarantees (I4) for `CatalogsConfig` (`BTreeMap`, not `HashMap`)
refined-by:
  - 15 (`foundations/15_mapping_and_binding.md` — how `CatalogRef` participates in the binding process)
  - 31b (`apis/31b_semstrait_core_io.md` — transport vocabulary used by the §5.4 wrappers)
  - 33 (`apis/33_semstrait_manifest.md` — catalog resolution at compile time)
  - 37 (`apis/37_semstrait_catalog.md` — the `CatalogProvider` trait that consumes a resolved catalog connection)
---

# 32b. `catalogs.yaml` — Catalog Grammar

`32b` fixes the grammar of the sibling `catalogs.yaml` file. A model references a catalog by alias via `extras.catalog:` (authored per `32 §1.3`); the catalog entry itself lives here.

## 1. Root YAML Shape

```yaml
# catalogs.yaml
catalogs:
  polaris_prod:
    type: polaris
    name: fs1henp3k8p1hqo
    url: https://polaris.example.com/api/catalog
    realm: POLARIS
    default_namespace: finance
    auth:
      method: aws_secrets
      secret_arn: arn:aws:secretsmanager:eu-west-1:123456:secret:polaris
      region: eu-west-1
      scope: "PRINCIPAL_ROLE:ALL"

  polaris_dev:
    type: polaris
    name: dev_catalog
    url: https://polaris-dev.example.com/api/catalog
    auth:
      method: oauth2
      client_id: ${POLARIS_CLIENT_ID}
      client_secret: ${POLARIS_CLIENT_SECRET}
      scope: "PRINCIPAL_ROLE:ALL"

  iceberg_local:
    type: iceberg_rest
    name: local_catalog
    url: http://localhost:8181
    auth:
      method: bearer
      token: ${LOCAL_ICEBERG_TOKEN}
```

Root shape: exactly one top-level `catalogs:` key. No other top-level keys are recognized.

Rust root:

```rust
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogsConfig {
    pub catalogs: BTreeMap<String, CatalogEntry>,
}
```

`catalogs` is a `BTreeMap` for I4 (deterministic iteration). `deny_unknown_fields` at the root rejects typos in the top-level key.

---

## 2. `CatalogEntry` Fields

```rust
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogEntry {
    /// Provider type discriminator (e.g. "polaris", "iceberg_rest", "unity").
    /// Free-form string; provider implementations live under feature-gated
    /// modules in `semstrait-catalog` (per `37`).
    #[serde(rename = "type")]
    pub provider_type: String,

    /// Catalog / warehouse name as the provider expects it.
    pub name: String,

    /// Catalog base URL.
    pub url: String,

    /// Provider-specific realm (Polaris uses this; most others ignore).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realm: Option<String>,

    /// Default namespace used when the model's `CatalogRef` does not
    /// override it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_namespace: Option<String>,

    /// Authentication configuration — see §3.
    pub auth: CatalogAuthMethod,
}
```

### 2.1 Optional vs required

- Required: `type`, `name`, `url`, `auth`.
- Optional: `realm`, `default_namespace`.

Missing required fields at parse → `parse.catalog-missing-field { alias, field }`.

### 2.2 `type:` discriminator

A free-form string rather than a closed enum. Adding a provider is a new feature-gated module in `semstrait-catalog` (per `37`); the model-layer grammar does not need to change when a new provider lands.

Unknown `type:` values pass the model-layer parser; the error surfaces at catalog-resolution time (`33` / `37`) as `CAT_E_0xxx UnknownProviderType` when the caller tries to open a provider the runtime hasn't linked in.

---

## 3. Authentication Methods

```rust
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
pub enum CatalogAuthMethod {
    Oauth2 {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token_url: Option<String>,
        client_id: String,
        client_secret: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
    },
    Bearer {
        token: String,
    },
    AwsSecrets {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token_url: Option<String>,
        secret_arn: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        region: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        aws_access_key_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        aws_secret_access_key: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        aws_session_token: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        aws_profile: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        secret_keys: Option<SecretKeyMapping>,
    },
}
```

### 3.1 `oauth2`

Direct OAuth2 with explicit credentials (or via `${VAR}`).

```yaml
auth:
  method: oauth2
  token_url: https://auth.example.com/oauth/token   # optional; derived from url if absent
  client_id: my_client
  client_secret: ${MY_SECRET}
  scope: "PRINCIPAL_ROLE:ALL"
```

Required: `client_id`, `client_secret`. Optional: `token_url`, `scope`.

### 3.2 `bearer`

Static token.

```yaml
auth:
  method: bearer
  token: ${ACCESS_TOKEN}
```

Required: `token`.

### 3.3 `aws_secrets`

AWS Secrets Manager retrieves `client_id` / `client_secret`, then falls back to the OAuth2 flow.

```yaml
auth:
  method: aws_secrets
  secret_arn: arn:aws:secretsmanager:eu-west-1:123456:secret:polaris
  region: eu-west-1
  scope: "PRINCIPAL_ROLE:ALL"
  secret_keys:
    client_id_key: polaris_client_id
    client_secret_key: polaris_client_secret
```

Required: `secret_arn`. Everything else optional.

`secret_keys` maps custom key names inside a Secrets Manager JSON payload:

```rust
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SecretKeyMapping {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id_key: Option<String>,      // default: "client_id"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret_key: Option<String>,  // default: "client_secret"
}
```

---

## 4. `CatalogRef` — Reference Site Inside a Model

Inside any `extras.catalog:` block in a model (authored per `32 §1.3`), the value is a `CatalogRef` — a **bare alias string** wrapped in a transparent newtype. There is no namespace override at the reference site, no map form, no additional fields:

```rust
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct CatalogRef {
    /// Alias — keys a `CatalogEntry` in `catalogs.yaml`.
    pub alias: String,
}
```

Only one authoring form exists:

```yaml
extras:
  catalog: polaris_prod
# → CatalogRef { alias: "polaris_prod" }
```

Map-form authoring (`{ alias: ..., namespace: ... }`) is **not recognized** — models author at most one catalog per data kind, and each catalog already declares its effective `default_namespace` at its own entry in `catalogs.yaml`. A model that needs a different namespace authors a separate catalog entry (typically a lightweight dev / prod variant pair).

### 4.1 Resolution precedence

At `compile` time, every `CatalogRef` is matched against `CatalogsConfig.catalogs[alias]`. Unmatched aliases raise `compile.catalog-alias-not-found` (per `33`). Namespace precedence collapses to two tiers:

1. `CatalogEntry.default_namespace` — authored at the catalog entry in `catalogs.yaml` (§2).
2. Provider default — whatever the provider implementation returns when both of the above are absent.

### 4.2 Why no reference-site namespace override

Considered and dropped. Reasons:
- **Single source of truth.** The catalog knows which namespace is canonical for it; the model's reference site should only select *which* catalog, not override its namespace.
- **Simpler mental model.** One alias, one catalog, one namespace — predictable at a glance. No hunting through `extras` trees to find the "effective" namespace.
- **Future extension.** If a legitimate use case emerges (e.g. per-DataKind tenant routing), it can be added as a new extras field at that layer, keeping `CatalogRef` shape stable.

---

## 5. Loader Contract

### 5.1 Signature

```rust
/// Parse a `catalogs.yaml` document. Pure and synchronous.
/// `${VAR}` substitutions expanded before YAML decoding (§6).
pub fn parse_catalogs(input: &str) -> Result<CatalogsConfig, CatalogsParseError>;
```

Pure: same input + same environment → byte-identical output under a canonical serializer.

### 5.2 Error enum

```rust
#[non_exhaustive]
pub enum CatalogsParseError {
    YamlSyntax             { message: String, location: Option<Location> },
    UnsetEnvVar            { var: String, location: Option<Location> },
    UnknownTopLevelKey     { key: String, location: Option<Location> },
    CatalogMissingField    { alias: String, field: String, location: Option<Location> },
    MalformedAuthMethod    { alias: String, reason: String, location: Option<Location> },
    UnknownField           { field: String, parent: String, location: Option<Location> },
}
```

Stable codes: kebab-case, e.g. `"parse.catalog-missing-field"`. Per `30 §6`.

### 5.3 Composition with `parse`

`parse_catalogs` is distinct from `parse` (the `semantic_model:` loader, `32 §9`). A caller typically invokes both, then hands the two products to `compile`:

```rust
let model     = semstrait_model::parse(&model_yaml)?;
let catalogs  = semstrait_model::parse_catalogs(&catalogs_yaml)?;
let manifest  = semstrait_manifest::compile(&model, &catalogs)?;
```

The exact shape of `compile` is `33`'s to pin down.

### 5.4 Async load / dump wrappers (shared with the model)

The ergonomic "read-then-parse" and "serialize-then-write" forms live on `semstrait-model::io` (`32 §10.4`) alongside `load_model` / `dump_model`. Catalog callers consume the same transport vocabulary (`Source` / `Sink` / `Location` per `31b`):

```rust
use semstrait_core::io::Location;
use semstrait_model::io::{load_catalogs, dump_catalogs, DumpMode};

let loc = "./catalogs.yaml".parse::<Location>()?;
let catalogs = load_catalogs(&loc).await?;

// ... edits ...

dump_catalogs(&catalogs, &loc, DumpMode::Canonical).await?;
```

`load_catalogs` composes `src.read::<String>().await` with `parse_catalogs`; `dump_catalogs` composes a canonical render with `sink.write(canonical).await`. Error roster (`CatalogsLoadError` / `CatalogsDumpError`) is defined in `32 §10.4.2` and is `#[non_exhaustive]` per `31b §7`. The wrappers are gated behind `semstrait-model`'s `io` feature (default off) per `32 §10.5`.

---

## 6. Environment-Variable Substitution

`${IDENT}` tokens in any string field are rewritten to the value of `std::env::var("IDENT")` before YAML decoding. Identical mechanism to `32 §8`; unset variables raise `parse.unset-env-var`.

```yaml
catalogs:
  polaris_prod:
    type: polaris
    name: ${POLARIS_CATALOG_NAME}
    url: ${POLARIS_URL}
    auth:
      method: bearer
      token: ${POLARIS_TOKEN}
```

Bare `$VAR` is treated as literal text.

---

## 7. Structural Rules

| ID | Rule | Diagnostic |
|---|---|---|
| **SR-C1** | Exactly one `catalogs:` root key; no other top-level keys. | `parse.unknown-top-level-key` |
| **SR-C2** | Every `CatalogEntry` carries required fields: `type`, `name`, `url`, `auth`. | `parse.catalog-missing-field` |
| **SR-C3** | Every `CatalogAuthMethod` variant carries its variant-specific required fields (e.g. `bearer` must have `token`; `aws_secrets` must have `secret_arn`). | `parse.malformed-auth-method` |
| **SR-C4** | Aliases (keys in `catalogs:`) are unique — `BTreeMap` enforces. | (duplicate-key raised by YAML decoder) |
| **SR-C5** | `deny_unknown_fields` applied to `CatalogsConfig`, `CatalogEntry`, every `CatalogAuthMethod` variant, and `SecretKeyMapping`. | `parse.unknown-field` |
| **SR-C6** | `${VAR}` substitution applied before decoding; unset vars fatal. | `parse.unset-env-var` |

---

*Cross-references use `NN §M.K` for internal sections and full relative paths for other docs.*
