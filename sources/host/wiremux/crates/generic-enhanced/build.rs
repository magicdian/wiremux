use std::env;
use std::fs::File;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let api_dir = manifest_dir.join("../../../../api/host/generic_enhanced/versions/current");
    let proto = api_dir.join("generic_enhanced.proto");
    let catalog = api_dir.join("catalog.textproto");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let catalog_bin = out_dir.join("generic_enhanced_catalog.pb");

    prost_build::Config::new()
        .compile_protos(std::slice::from_ref(&proto), std::slice::from_ref(&api_dir))
        .expect("failed to compile generic enhanced proto");

    let input = File::open(&catalog).expect("failed to open generic enhanced catalog");
    let output =
        File::create(&catalog_bin).expect("failed to create encoded generic enhanced catalog");
    let status = Command::new("protoc")
        .arg("--encode=wiremux.host.generic_enhanced.v1.GenericEnhancedApiCatalog")
        .arg(format!("--proto_path={}", api_dir.display()))
        .arg(&proto)
        .stdin(Stdio::from(input))
        .stdout(Stdio::from(output))
        .status()
        .expect("failed to run protoc for generic enhanced catalog");
    if !status.success() {
        panic!("protoc failed to encode generic enhanced catalog with status {status}");
    }

    println!("cargo:rerun-if-changed={}", proto.display());
    println!("cargo:rerun-if-changed={}", catalog.display());
}
