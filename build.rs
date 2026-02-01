fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_file = "coreengine/proto/engine.proto";

    println!("cargo:rerun-if-changed={}", proto_file);

    // Configure tonic code generation (outputs to OUT_DIR by default)
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&[proto_file], &["coreengine/proto"])?;

    Ok(())
}
