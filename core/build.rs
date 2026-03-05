fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("CARGO_FEATURE_GRPC").is_ok() {
        tonic_build::configure()
            .build_server(false)
            .compile_protos(&["proto/sidecar.proto"], &["proto"])?;
    }
    Ok(())
}
