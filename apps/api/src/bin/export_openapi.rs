#![forbid(unsafe_code)]

//! Build-time export of the OpenAPI spec for the 17 safe endpoints
//! (security-fuzzing spec, `.scratch/security-fuzzing/`). Writes pretty JSON
//! to `security/openapi.json` (default) without starting the server — the spec
//! is never served at runtime. The committed artifact is guarded by the
//! drift-check test in `crate::openapi`.
//!
//! Usage (from `apps/api`):
//!   cargo run --bin export_openapi [-- <out-path>]

use openworkspace_api::openapi::export_json;

fn main() {
    let out_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "security/openapi.json".to_string());

    let pretty = serde_json::to_string_pretty(&export_json()).expect("spec must serialize");

    std::fs::write(&out_path, format!("{pretty}\n")).unwrap_or_else(|e| {
        eprintln!("failed to write OpenAPI spec to {out_path}: {e}");
        std::process::exit(1);
    });

    println!("OpenAPI spec written to {out_path}");
}
