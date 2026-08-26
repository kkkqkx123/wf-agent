fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "grpc")]
    {
        tonic_build::compile_protos("src/api/rpc/proto/layertwine.proto")?;
    }
    Ok(())
}
