//! Embeds everything under `content/` into the binary, so the wasm module
//! carries the whole site and needs no fetches after it loads.

use std::{env, fs, path::Path};

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("content");
    println!("cargo:rerun-if-changed=content");

    let mut files = Vec::new();
    walk(&root, &root, &mut files);
    files.sort();

    let mut out = String::from("pub static FILES: &[(&str, &str)] = &[\n");
    for (rel, abs) in files {
        out += &format!("    ({rel:?}, include_str!({abs:?})),\n");
    }
    out += "];\n";
    fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("content.rs"), out).unwrap();
}

fn walk(root: &Path, dir: &Path, files: &mut Vec<(String, String)>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, files);
        } else if path.extension().is_some_and(|e| e == "md") {
            let rel = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            files.push((rel, path.to_string_lossy().into_owned()));
        }
    }
}
