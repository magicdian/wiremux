use std::env;
use std::fs::File;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let api_dir =
        manifest_dir.join("../../../../api/host/vendor_enhanced/espressif/versions/current");
    let proto = api_dir.join("espressif_vendor_enhanced.proto");
    let catalog = api_dir.join("catalog.textproto");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let catalog_bin = out_dir.join("espressif_vendor_enhanced_catalog.pb");

    prost_build::Config::new()
        .compile_protos(&[proto.clone()], &[api_dir.clone()])
        .expect("failed to compile Espressif vendor enhanced proto");

    let input = File::open(&catalog).expect("failed to open Espressif vendor enhanced catalog");
    let output = File::create(&catalog_bin)
        .expect("failed to create encoded Espressif vendor enhanced catalog");
    let status = Command::new("protoc")
        .arg("--encode=wiremux.host.vendor_enhanced.espressif.v1.EspressifVendorEnhancedApiCatalog")
        .arg(format!("--proto_path={}", api_dir.display()))
        .arg(&proto)
        .stdin(Stdio::from(input))
        .stdout(Stdio::from(output))
        .status()
        .expect("failed to run protoc for Espressif vendor enhanced catalog");
    if !status.success() {
        panic!("protoc failed to encode Espressif vendor enhanced catalog with status {status}");
    }

    println!("cargo:rerun-if-changed={}", proto.display());
    println!("cargo:rerun-if-changed={}", catalog.display());
}
