fn main() -> std::io::Result<()> {
    prost_build::compile_protos(&["examples/example.proto"], &["examples/"])?;
    Ok(())
}
