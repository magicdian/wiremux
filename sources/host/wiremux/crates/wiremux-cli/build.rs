use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let core_dir = manifest_dir.join("../../../../core/c");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let include_dir = core_dir.join("include");
    let sources = [
        "src/wiremux_batch.c",
        "src/wiremux_compression.c",
        "src/wiremux_envelope.c",
        "src/wiremux_frame.c",
        "src/wiremux_host_session.c",
        "src/wiremux_manifest.c",
        "src/wiremux_version.c",
    ];

    let mut objects = Vec::new();
    for source in sources {
        let source_path = core_dir.join(source);
        let object_path = out_dir.join(format!("{}.o", source.replace('/', "_").replace(".c", "")));
        run(Command::new("cc")
            .arg("-std=c99")
            .arg("-I")
            .arg(&include_dir)
            .arg("-Wall")
            .arg("-Wextra")
            .arg("-Werror")
            .arg("-c")
            .arg(&source_path)
            .arg("-o")
            .arg(&object_path));
        objects.push(object_path);
        println!("cargo:rerun-if-changed={}", source_path.display());
    }

    let library = out_dir.join("libwiremux_core_c.a");
    let mut ar = Command::new("ar");
    ar.arg("crus").arg(&library);
    for object in &objects {
        ar.arg(object);
    }
    run(&mut ar);

    println!("cargo:rerun-if-changed={}", include_dir.display());
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=wiremux_core_c");
}

fn run(command: &mut Command) {
    let status = command.status().expect("failed to run build command");
    if !status.success() {
        panic!("build command failed with status {status}: {:?}", command);
    }
}
