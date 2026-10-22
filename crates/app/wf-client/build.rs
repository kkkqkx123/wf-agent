//! Build-time OpenAPI codegen for wf-client.
//!
//! Reads the committed `wf-server/openapi.json` snapshot (OpenAPI 3.1),
//! normalizes an in-memory copy down to 3.0 (see `src/normalize.rs`, shared
//! via `#[path]` so the transform is unit-tested), then runs progenitor and
//! writes the generated client to `OUT_DIR/codegen.rs` for `include!`.

#[path = "src/normalize.rs"]
mod normalize;

fn main() {
    let src = "../wf-server/openapi.json";
    println!("cargo:rerun-if-changed={src}");
    // normalize.rs is pulled in via #[path] below; once rerun-if-changed is
    // declared Cargo stops tracking the rest of the package, so list it.
    println!("cargo:rerun-if-changed=src/normalize.rs");

    let text = std::fs::read_to_string(src).expect("read wf-server openapi.json snapshot");
    let mut value: serde_json::Value =
        serde_json::from_str(&text).expect("openapi snapshot must be valid JSON");
    normalize::normalize_document(&mut value);

    let spec: openapiv3::OpenAPI =
        serde_json::from_value(value).expect("normalized snapshot must parse as OpenAPI 3.0");

    let mut generator = progenitor::Generator::default();
    let tokens = generator
        .generate_tokens(&spec)
        .expect("progenitor must accept the normalized snapshot");
    let ast = syn::parse2(tokens).expect("generated tokens must parse");
    let content = prettyplease::unparse(&ast);

    let mut out_file = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    out_file.push("codegen.rs");
    std::fs::write(&out_file, content).expect("write codegen.rs");
}
