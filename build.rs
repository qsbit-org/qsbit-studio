use std::{env, fs, path::PathBuf};
fn main() {
    println!("cargo:rerun-if-changed=presets/studio-ideal-v1.json");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let directory = output.ancestors().nth(3).unwrap().join("presets");
    fs::create_dir_all(&directory).unwrap();
    fs::copy(
        "presets/studio-ideal-v1.json",
        directory.join("studio-ideal-v1.json"),
    )
    .unwrap();
}
