use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Deserialize)]
struct Config {
    defaults: Defaults,
    devices: BTreeMap<String, DeviceDef>,
    host_presets: BTreeMap<String, HostPresetDef>,
    tools: BTreeMap<String, ToolContract>,
}

#[derive(Debug, Deserialize)]
struct Defaults {
    device: String,
    host_preset: String,
}

#[derive(Debug, Deserialize)]
struct DeviceDef {
    product: String,
}

#[derive(Debug, Deserialize)]
struct HostPresetDef {
    profile: String,
}

#[derive(Debug, Deserialize)]
struct ToolContract {
    expected_contains: String,
    ci_strict: bool,
}

#[derive(Debug, Deserialize, Serialize, Default)]
struct Selected {
    device: String,
    host_preset: String,
    product: Option<String>,
    profile: Option<String>,
    selected_at_unix: Option<u64>,
}

#[derive(Debug, Serialize)]
struct MetadataRecord {
    event: String,
    unix: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ci: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dirty: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    idf_py: Option<bool>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let repo_root = repo_root()?;
    let config_path = repo_root.join("build/wiremux-build.toml");
    let selected_path = repo_root.join(".wiremux/build/selected.toml");
    let metadata_path = repo_root.join("build/out/metadata.jsonl");
    let config = read_config(&config_path)?;

    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        return Err(usage());
    }

    match args[0].as_str() {
        "lunch" => cmd_lunch(&repo_root, &config, &selected_path, &args[1..]),
        "env" => cmd_env(&config, &selected_path, &args[1..]),
        "doctor" => cmd_doctor(&repo_root, &config, &metadata_path),
        "check" => cmd_check(&repo_root, &config, &metadata_path, &args[1..]),
        "build" => cmd_build(&repo_root, &config, &metadata_path, &args[1..]),
        "package" => cmd_package(&repo_root, &config, &metadata_path, &args[1..]),
        _ => Err(usage()),
    }
}

fn usage() -> String {
    "usage: wiremux-build <command>\n  lunch <device> <host-preset>\n  env --shell bash|zsh\n  doctor\n  check core|host|vendor-espressif|all\n  build core|host|vendor-espressif\n  package esp-registry".to_string()
}

fn repo_root() -> Result<PathBuf, String> {
    let helper_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = helper_dir
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "failed to resolve repository root".to_string())?;
    Ok(root.to_path_buf())
}

fn read_config(path: &Path) -> Result<Config, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    toml::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))
}

fn read_selected(path: &Path) -> Result<Option<Selected>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let selected: Selected =
        toml::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))?;
    Ok(Some(selected))
}

fn write_selected(path: &Path, selected: &Selected) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    let body = toml::to_string(selected).map_err(|e| format!("serialize selected: {e}"))?;
    fs::write(path, body).map_err(|e| format!("write {}: {e}", path.display()))
}

fn cmd_lunch(
    _repo_root: &Path,
    config: &Config,
    selected_path: &Path,
    args: &[String],
) -> Result<(), String> {
    if args.len() != 2 {
        return Err("lunch requires <device> <host-preset>".to_string());
    }
    let device = &args[0];
    let host_preset = &args[1];

    if device == "core-only" && host_preset == "device-only" {
        return Err("invalid selection: core-only cannot be paired with device-only".to_string());
    }

    let device_def = config
        .devices
        .get(device)
        .ok_or_else(|| format!("unknown device: {device}"))?;
    let host_def = config
        .host_presets
        .get(host_preset)
        .ok_or_else(|| format!("unknown host preset: {host_preset}"))?;

    let selected = Selected {
        device: device.clone(),
        host_preset: host_preset.clone(),
        product: Some(device_def.product.clone()),
        profile: Some(host_def.profile.clone()),
        selected_at_unix: Some(now_unix()),
    };
    write_selected(selected_path, &selected)?;
    println!("selected device={device} host_preset={host_preset}");
    Ok(())
}

fn cmd_env(config: &Config, selected_path: &Path, args: &[String]) -> Result<(), String> {
    if args.len() != 2 || args[0] != "--shell" {
        return Err("env requires --shell bash|zsh".to_string());
    }
    let shell = args[1].as_str();
    if shell != "bash" && shell != "zsh" {
        return Err("env --shell supports only bash|zsh".to_string());
    }

    let resolved = resolve_selected(config, selected_path, None, None)?;
    println!("export WIREMUX_DEVICE='{}'", resolved.device);
    println!("export WIREMUX_HOST_PRESET='{}'", resolved.host_preset);
    if let Some(product) = resolved.product {
        println!("export WIREMUX_PRODUCT='{}'", product);
    }
    if let Some(profile) = resolved.profile {
        println!("export WIREMUX_PROFILE='{}'", profile);
    }
    Ok(())
}

fn cmd_doctor(repo_root: &Path, config: &Config, metadata_path: &Path) -> Result<(), String> {
    let ci = is_ci();
    let dirty = git_dirty(repo_root).unwrap_or(false);

    check_tool_contract("python3", &["--version"], config.tools.get("python"), ci)?;
    check_tool_contract("rustc", &["--version"], config.tools.get("rustc"), ci)?;
    check_tool_contract("cargo", &["--version"], config.tools.get("cargo"), ci)?;
    check_tool_contract("cmake", &["--version"], config.tools.get("cmake"), ci)?;
    check_tool_contract("ctest", &["--version"], config.tools.get("ctest"), ci)?;

    let idf_ok = check_optional_tool_contract("idf.py", &["--version"], config.tools.get("idf_py"), ci)?;

    append_metadata(
        metadata_path,
        &MetadataRecord {
            event: "doctor".to_string(),
            unix: now_unix(),
            target: None,
            status: Some("ok".to_string()),
            ci: Some(ci),
            dirty: Some(dirty),
            idf_py: Some(idf_ok),
        },
    )?;
    Ok(())
}

fn cmd_check(
    repo_root: &Path,
    config: &Config,
    metadata_path: &Path,
    args: &[String],
) -> Result<(), String> {
    let ci = is_ci();
    if args.len() != 1 {
        return Err("check requires core|host|vendor-espressif|all".to_string());
    }
    match args[0].as_str() {
        "core" => run_core_check(repo_root)?,
        "host" => run_host_check(repo_root)?,
        "vendor-espressif" => run_vendor_build(repo_root, config.tools.get("idf_py"), ci, true)?,
        "all" => {
            run_core_check(repo_root)?;
            run_host_check(repo_root)?;
            run_vendor_build(repo_root, config.tools.get("idf_py"), ci, true)?;
        }
        _ => return Err("check requires core|host|vendor-espressif|all".to_string()),
    }
    append_metadata(metadata_path, &MetadataRecord {
        event: "check".to_string(),
        unix: now_unix(),
        target: Some(args[0].clone()),
        status: Some("ok".to_string()),
        ci: None,
        dirty: None,
        idf_py: None,
    })?;
    Ok(())
}

fn cmd_build(
    repo_root: &Path,
    config: &Config,
    metadata_path: &Path,
    args: &[String],
) -> Result<(), String> {
    let ci = is_ci();
    if args.len() != 1 {
        return Err("build requires core|host|vendor-espressif".to_string());
    }
    match args[0].as_str() {
        "core" => run_core_build(repo_root)?,
        "host" => run_host_build(repo_root)?,
        "vendor-espressif" => run_vendor_build(repo_root, config.tools.get("idf_py"), ci, false)?,
        _ => return Err("build requires core|host|vendor-espressif".to_string()),
    }
    append_metadata(metadata_path, &MetadataRecord {
        event: "build".to_string(),
        unix: now_unix(),
        target: Some(args[0].clone()),
        status: Some("ok".to_string()),
        ci: None,
        dirty: None,
        idf_py: None,
    })?;
    Ok(())
}

fn cmd_package(
    repo_root: &Path,
    _config: &Config,
    metadata_path: &Path,
    args: &[String],
) -> Result<(), String> {
    if args.len() != 1 || args[0] != "esp-registry" {
        return Err("package requires esp-registry".to_string());
    }
    run_native(
        repo_root,
        "tools/esp-registry/generate-packages.sh",
        &[],
        None,
    )?;
    append_metadata(metadata_path, &MetadataRecord {
        event: "package".to_string(),
        unix: now_unix(),
        target: Some("esp-registry".to_string()),
        status: Some("ok".to_string()),
        ci: None,
        dirty: None,
        idf_py: None,
    })?;
    Ok(())
}

fn resolve_selected(
    config: &Config,
    selected_path: &Path,
    cli_device: Option<String>,
    cli_host: Option<String>,
) -> Result<Selected, String> {
    let selected = read_selected(selected_path)?;
    let selected_device = selected.as_ref().map(|s| s.device.clone());
    let selected_host = selected.as_ref().map(|s| s.host_preset.clone());

    let device = cli_device
        .or(selected_device)
        .unwrap_or_else(|| config.defaults.device.clone());
    let host_preset = cli_host
        .or(selected_host)
        .unwrap_or_else(|| config.defaults.host_preset.clone());

    let device_def = config
        .devices
        .get(&device)
        .ok_or_else(|| format!("unknown device: {device}"))?;
    let host_def = config
        .host_presets
        .get(&host_preset)
        .ok_or_else(|| format!("unknown host preset: {host_preset}"))?;

    Ok(Selected {
        device,
        host_preset,
        product: Some(device_def.product.clone()),
        profile: Some(host_def.profile.clone()),
        selected_at_unix: selected.and_then(|s| s.selected_at_unix),
    })
}

fn run_core_check(repo_root: &Path) -> Result<(), String> {
    run_native(
        repo_root,
        "cmake",
        &["-S", "sources/core/c", "-B", "sources/core/c/build"],
        None,
    )?;
    run_native(repo_root, "cmake", &["--build", "sources/core/c/build"], None)?;
    run_native(
        repo_root,
        "ctest",
        &["--test-dir", "sources/core/c/build", "--output-on-failure"],
        None,
    )?;
    Ok(())
}

fn run_core_build(repo_root: &Path) -> Result<(), String> {
    run_native(
        repo_root,
        "cmake",
        &["-S", "sources/core/c", "-B", "sources/core/c/build"],
        None,
    )?;
    run_native(repo_root, "cmake", &["--build", "sources/core/c/build"], None)?;
    Ok(())
}

fn run_host_check(repo_root: &Path) -> Result<(), String> {
    let host_dir = repo_root.join("sources/host/wiremux");
    run_native(&host_dir, "cargo", &["fmt", "--check"], None)?;
    run_native(&host_dir, "cargo", &["check"], None)?;
    run_native(&host_dir, "cargo", &["test"], None)?;
    Ok(())
}

fn run_host_build(repo_root: &Path) -> Result<(), String> {
    let host_dir = repo_root.join("sources/host/wiremux");
    run_native(&host_dir, "cargo", &["build"], None)?;
    Ok(())
}

fn run_vendor_build(
    repo_root: &Path,
    idf_contract: Option<&ToolContract>,
    ci: bool,
    local_skip_ok: bool,
) -> Result<(), String> {
    let idf_ok = check_optional_tool_contract("idf.py", &["--version"], idf_contract, ci)?;
    if !idf_ok {
        if local_skip_ok && !ci {
            println!("skip: idf.py not found; vendor-espressif check skipped for local environment");
            return Ok(());
        }
        return Err("idf.py not found; cannot build vendor-espressif".to_string());
    }
    let vendor_dir =
        repo_root.join("sources/vendor/espressif/generic/examples/esp_wiremux_console_demo");
    run_native(&vendor_dir, "idf.py", &["build"], None)?;
    Ok(())
}

fn check_optional_tool_contract(
    label: &str,
    args: &[&str],
    contract: Option<&ToolContract>,
    ci: bool,
) -> Result<bool, String> {
    match run_capture(label, args) {
        Ok((true, text)) => {
            println!("doctor: {label} {text}");
            if let Some(c) = contract {
                if !text.contains(&c.expected_contains) {
                    let msg = format!(
                        "doctor: {label} version mismatch; expected to contain '{}', got '{}'",
                        c.expected_contains, text
                    );
                    if ci && c.ci_strict {
                        return Err(msg);
                    }
                    eprintln!("warning: {msg}");
                }
            }
            Ok(true)
        }
        _ => {
            let msg = format!("doctor: {label} missing");
            if let Some(c) = contract {
                if ci && c.ci_strict {
                    return Err(msg);
                }
            }
            eprintln!("warning: {msg}");
            Ok(false)
        }
    }
}

fn run_native(cwd: &Path, program: &str, args: &[&str], extra_env: Option<(&str, &str)>) -> Result<(), String> {
    let cmdline = std::iter::once(program.to_string())
        .chain(args.iter().map(|s| s.to_string()))
        .collect::<Vec<String>>()
        .join(" ");
    println!("+ {cmdline}");

    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(cwd);
    if let Some((k, v)) = extra_env {
        cmd.env(k, v);
    }
    let status = cmd.status().map_err(|e| format!("spawn {program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("command failed: {cmdline} (status {status})"))
    }
}

fn run_capture(program: &str, args: &[&str]) -> io::Result<(bool, String)> {
    let output = Command::new(program).args(args).output()?;
    let ok = output.status.success();
    let mut text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        text = String::from_utf8_lossy(&output.stderr).trim().to_string();
    }
    Ok((ok, text))
}

fn check_tool_contract(
    label: &str,
    args: &[&str],
    contract: Option<&ToolContract>,
    ci: bool,
) -> Result<(), String> {
    let (ok, text) = run_capture(label, args).map_err(|e| format!("run {label}: {e}"))?;
    if !ok {
        return Err(format!("doctor: {label} command failed"));
    }
    println!("doctor: {label} {text}");
    if let Some(c) = contract {
        if !text.contains(&c.expected_contains) {
            let msg = format!(
                "doctor: {label} version mismatch; expected to contain '{}', got '{}'",
                c.expected_contains, text
            );
            if ci && c.ci_strict {
                return Err(msg);
            }
            eprintln!("warning: {msg}");
        }
    }
    Ok(())
}

fn is_ci() -> bool {
    env::var("CI").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false)
}

fn git_dirty(repo_root: &Path) -> Result<bool, String> {
    let output = Command::new("git")
        .arg("status")
        .arg("--porcelain")
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git status --porcelain: {e}"))?;
    if !output.status.success() {
        return Err("git status --porcelain failed".to_string());
    }
    Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

fn append_metadata(path: &Path, record: &MetadataRecord) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    let line = serde_json::to_string(record).map_err(|e| format!("serialize metadata: {e}"))?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("open {}: {e}", path.display()))?;
    file.write_all(line.as_bytes())
        .map_err(|e| format!("append {}: {e}", path.display()))?;
    file.write_all(b"\n")
        .map_err(|e| format!("append newline {}: {e}", path.display()))
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
