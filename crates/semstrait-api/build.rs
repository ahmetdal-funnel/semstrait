fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "grpc")]
    {
        tonic_prost_build::compile_protos("../../proto/semstrait/v1/service.proto")?;
    }
    Ok(())
}
