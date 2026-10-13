// SPDX-License-Identifier: Apache-2.0

fn main() {
    // ONNX Runtime's Linux binary needs libstdc++. rust-lld does not search GCC's private directory.
    let root = std::path::Path::new("/usr/lib/gcc/x86_64-linux-gnu");
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.join("libstdc++.so").is_file() {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
    }
}
