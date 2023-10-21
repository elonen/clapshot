fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{path::PathBuf, env};

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../proto");
    let proto_files = vec![root.join("clapshot.proto")];

    let descriptor_path = PathBuf::from(env::var("OUT_DIR").unwrap())
        .join("clapshot_descriptor.bin");

    tonic_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional")
        .file_descriptor_set_path(&descriptor_path)
        .compile_well_known_types(true)
        .extern_path(".google.protobuf", "::pbjson_types")
        .compile(&proto_files, &[root.clone()])
        .unwrap();

    let descriptor_set = std::fs::read(descriptor_path)?;
    pbjson_build::Builder::new()
        .register_descriptors(&descriptor_set)?
        .build(&[".clapshot"])?;

    Ok(())
}
