use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let src = manifest_dir.join("src/data/litellm_public.json");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let dest = out_dir.join("litellm_public.json");

    let contents = fs::read_to_string(&src).unwrap_or_else(|_| "{}".to_string());
    fs::write(&dest, contents).expect("failed to write embedded pricing table");

    println!("cargo:rerun-if-changed=src/data/litellm_public.json");
}
