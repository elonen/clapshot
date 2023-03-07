const ORGANIZER_PROTO: &str = "src/grpc/organizer.proto";


fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{path::PathBuf, env};
    
    tonic_build::compile_protos(ORGANIZER_PROTO)?;

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    tonic_build::configure()
        .file_descriptor_set_path(out_dir.join("organizer_descriptor.bin"))
        .compile(&[ORGANIZER_PROTO], &["proto"])
        .unwrap();

    Ok(())
}