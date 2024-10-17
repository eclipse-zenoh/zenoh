use std::{env, fs::File, io::Write, path::Path};

use which::which;

fn main() -> std::io::Result<()> {
    // If protoc is not installed, we cheat because building protoc from source
    // with protobuf-src is way too long
    if which("protoc").is_err() {
        const PROTO: &str = r#"#[derive(Clone, PartialEq, ::prost::Message)] pub struct Entity { #[prost(uint32, tag = "1")] pub id: u32, #[prost(string, tag = "2")] pub name: ::prost::alloc::string::String,}"#;
        let out_path = Path::new(&env::var("OUT_DIR").unwrap()).join("example.rs");
        File::create(out_path)?.write_all(PROTO.as_bytes())?;
        return Ok(());
    }
    prost_build::compile_protos(&["examples/example.proto"], &["examples/"])?;
    Ok(())
}
