const ORGANIZER_PROTO: &str = "src/grpc/organizer.proto";


fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{path::PathBuf, env};
    
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    tonic_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional")
        .file_descriptor_set_path(out_dir.join("organizer_descriptor.bin"))
        .compile(&[ORGANIZER_PROTO], &["proto"])
        .unwrap();

    Ok(())
}