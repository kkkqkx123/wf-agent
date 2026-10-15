//! Golden capture helper shared by every demo example.
//!
//! Each form writes under `outputs/<form>/` so reviewable text stays beside
//! the form that produced it. Library tests stay side-effect free; examples
//! are the single place that writes files.

use std::fs;
use std::path::PathBuf;

/// Form-scoped outputs directory rooted at the manifest dir, not CWD.
pub fn outputs_dir(form: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("outputs")
        .join(form)
}

/// Write `content` plus a trailing newline to `outputs/<form>/<name>.txt`.
pub fn emit(form: &str, name: &str, content: &str) {
    let dir = outputs_dir(form);
    fs::create_dir_all(&dir).expect("create outputs dir");
    let path = dir.join(format!("{name}.txt"));
    fs::write(&path, format!("{content}\n")).expect("write output file");
    eprintln!("wrote {}", path.display());
}
