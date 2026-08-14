use std::{fs, path::Path};

fn ensure_staged_helper(path: &str) {
    let path = Path::new(path);
    if path.exists() {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, []);
}

fn main() {
    println!("cargo:rerun-if-changed=../version");
    ensure_staged_helper("../packaging/staged/iran-split-helper");
    ensure_staged_helper("../packaging/staged/iran-split-helper.exe");
    tauri_build::build();
}
