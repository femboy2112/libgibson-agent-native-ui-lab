# Schemas

The versioned wire formats are defined by Rust types (serde) and mirrored by the Python builders,
so there is a single source of truth. This directory is a pointer, not a second definition.

| schema id | defining type | file |
| --- | --- | --- |
| `sai.evidence/v1` | `sai_core::evidence::EvidenceArtifact` | `crates/sai-core/src/evidence.rs` |
| `sai.receipt/v1` | `sai_core::receipt::RunReceipt` | `crates/sai-core/src/receipt.rs` |
| `sai.quotient/v1` | `sai_core::quotient::RecoveredQuotient` | `crates/sai-core/src/quotient.rs` |
| `sai.truth/v1` | `sai_core::truth::TruthTrack` | `crates/sai-core/src/truth.rs` |
| `sai.metrics/v1` | `sai_core::metrics::EvaluationReport` | `crates/sai-core/src/metrics.rs` |
| `sai.dataset/v1` | `sai_harness::dataset::Manifest` | `crates/sai-harness/src/dataset.rs` |
| `sai.mutations/v1` | `sai_harness::mutations` | `crates/sai-harness/src/mutations.rs` |

Python builders: `adapters/sai_evidence.py` (`SCHEMA`, `RECEIPT_SCHEMA`).

Print the schema ids the CLI knows:

```bash
cargo run -q -p sai-core --bin sai -- schema
```
