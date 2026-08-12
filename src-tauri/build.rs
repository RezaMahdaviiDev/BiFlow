fn main() {
    println!("cargo:rerun-if-changed=../version");
    tauri_build::build();
}
