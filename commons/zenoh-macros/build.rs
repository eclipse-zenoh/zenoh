use std::{env, ffi::OsString, fs::OpenOptions, io::Write, process::Command};

fn main() {
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
    let output = Command::new(rustc)
        .arg("-v")
        .arg("-V")
        .output()
        .expect("Couldn't get rustc version");
    let version_rs = std::path::PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("version.rs");
    let mut version_rs = OpenOptions::new()
        .create(true)
        .write(true)
        .open(version_rs)
        .unwrap();
    version_rs.write_all(&output.stdout).unwrap();
}
