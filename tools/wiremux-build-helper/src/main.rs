use dialoguer::{theme::ColorfulTheme, Select};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

const VENDOR_KIND_SKIP: &str = "skip";
const VENDOR_KIND_ALL: &str = "all";
const VENDOR_KIND_MODEL: &str = "model";

const HOST_GENERIC: &str = "generic";
const HOST_VENDOR_ENHANCED: &str = "vendor-enhanced";
const HOST_ALL_FEATURES: &str = "all-features";

#[derive(Debug, Deserialize)]
struct BuildConfig {
    defaults: Defaults,
    tools: BTreeMap<String, ToolContract>,
}

#[derive(Debug, Deserialize)]
struct Defaults {
    vendor: String,
    host: String,
}

#[derive(Debug, Deserialize)]
struct VendorConfig {
    vendors: Vec<VendorDef>,
}

#[derive(Clone, Debug, Deserialize)]
struct VendorDef {
    id: String,
    label: String,
    kind: String,
    #[serde(default)]
    family: Option<String>,
    #[serde(default)]
    idf_target: Option<String>,
    #[serde(default)]
    example_path: Option<String>,
    #[serde(default)]
    host_feature: Option<String>,
    #[serde(default)]
    implemented: bool,
    #[serde(default)]
    include_in_all: bool,
}

#[derive(Debug, Deserialize)]
struct HostConfig {
    hosts: Vec<HostDef>,
}

#[derive(Clone, Debug, Deserialize)]
struct HostDef {
    id: String,
    label: String,
    profile: String,
    allowed_vendor_kinds: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ToolContract {
    expected_contains: String,
    ci_strict: bool,
}

#[derive(Debug, Deserialize, Serialize, Default)]
struct Selected {
    vendor: String,
    host: String,
    vendor_kind: String,
    vendor_label: String,
    host_profile: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    vendor_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vendor_idf_target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vendor_example_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
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

#[derive(Debug)]
enum LunchRequest {
    Interactive,
    Explicit { vendor: String, host: String },
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
    let build_config_path = repo_root.join("build/wiremux-build.toml");
    let vendor_config_path = repo_root.join("build/wiremux-vendors.toml");
    let host_config_path = repo_root.join("build/wiremux-hosts.toml");
    let selected_path = repo_root.join(".wiremux/build/selected.toml");
    let metadata_path = repo_root.join("build/out/metadata.jsonl");
    let build_config: BuildConfig = read_toml(&build_config_path)?;
    let vendor_config: VendorConfig = read_toml(&vendor_config_path)?;
    let host_config: HostConfig = read_toml(&host_config_path)?;
    validate_configs(&build_config, &vendor_config, &host_config)?;

    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        return Err(usage());
    }

    match args[0].as_str() {
        "lunch" => cmd_lunch(
            &build_config,
            &vendor_config,
            &host_config,
            &selected_path,
            &args[1..],
        ),
        "env" => cmd_env(
            &build_config,
            &vendor_config,
            &host_config,
            &selected_path,
            &args[1..],
        ),
        "doctor" => cmd_doctor(&repo_root, &build_config, &metadata_path),
        "check" => cmd_check(
            &repo_root,
            &build_config,
            &vendor_config,
            &host_config,
            &selected_path,
            &metadata_path,
            &args[1..],
        ),
        "build" => cmd_build(
            &repo_root,
            &build_config,
            &vendor_config,
            &host_config,
            &selected_path,
            &metadata_path,
            &args[1..],
        ),
        "package" => cmd_package(&repo_root, &build_config, &metadata_path, &args[1..]),
        _ => Err(usage()),
    }
}

fn usage() -> String {
    "usage: wiremux-build <command>\n  lunch [--vendor <skip|all|model> --host <generic|vendor-enhanced|all-features>]\n  env --shell bash|zsh\n  doctor\n  check core|host|vendor|vendor-espressif|all\n  build core|host|vendor|vendor-espressif\n  package esp-registry".to_string()
}

fn repo_root() -> Result<PathBuf, String> {
    let helper_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = helper_dir
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "failed to resolve repository root".to_string())?;
    Ok(root.to_path_buf())
}

fn read_toml<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    toml::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))
}

fn read_selected(path: &Path) -> Result<Option<Selected>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let selected: Selected = toml::from_str(&text).map_err(|e| {
        format!(
            "parse {}; run `tools/wiremux-build lunch` to regenerate selected state: {e}",
            path.display()
        )
    })?;
    Ok(Some(selected))
}

fn write_selected(path: &Path, selected: &Selected) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    let body = toml::to_string(selected).map_err(|e| format!("serialize selected: {e}"))?;
    fs::write(path, body).map_err(|e| format!("write {}: {e}", path.display()))
}

fn validate_configs(
    build_config: &BuildConfig,
    vendor_config: &VendorConfig,
    host_config: &HostConfig,
) -> Result<(), String> {
    let mut vendor_ids = BTreeSet::new();
    for vendor in &vendor_config.vendors {
        if !vendor_ids.insert(vendor.id.as_str()) {
            return Err(format!("duplicate vendor id: {}", vendor.id));
        }
        match vendor.kind.as_str() {
            VENDOR_KIND_SKIP | VENDOR_KIND_ALL | VENDOR_KIND_MODEL => {}
            _ => {
                return Err(format!(
                    "vendor {} has unknown kind {}",
                    vendor.id, vendor.kind
                ))
            }
        }
    }

    let mut host_ids = BTreeSet::new();
    for host in &host_config.hosts {
        if !host_ids.insert(host.id.as_str()) {
            return Err(format!("duplicate host id: {}", host.id));
        }
    }

    find_vendor(vendor_config, &build_config.defaults.vendor)?;
    find_host(host_config, &build_config.defaults.host)?;
    validate_selection(
        vendor_config,
        host_config,
        &build_config.defaults.vendor,
        &build_config.defaults.host,
    )?;
    Ok(())
}

fn cmd_lunch(
    build_config: &BuildConfig,
    vendor_config: &VendorConfig,
    host_config: &HostConfig,
    selected_path: &Path,
    args: &[String],
) -> Result<(), String> {
    let (vendor_id, host_id) = match parse_lunch_args(args)? {
        LunchRequest::Interactive => {
            choose_lunch_interactively(build_config, vendor_config, host_config)?
        }
        LunchRequest::Explicit { vendor, host } => (vendor, host),
    };

    let selected = resolve_selection(
        vendor_config,
        host_config,
        &vendor_id,
        &host_id,
        Some(now_unix()),
    )?;
    write_selected(selected_path, &selected)?;
    println!("selected vendor={} host={}", selected.vendor, selected.host);
    Ok(())
}

fn parse_lunch_args(args: &[String]) -> Result<LunchRequest, String> {
    if args.is_empty() {
        return Ok(LunchRequest::Interactive);
    }

    let mut vendor = None;
    let mut host = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--vendor" => {
                if vendor.is_some() {
                    return Err("lunch received duplicate --vendor".to_string());
                }
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| "lunch --vendor requires a value".to_string())?;
                vendor = Some(value.clone());
            }
            "--host" => {
                if host.is_some() {
                    return Err("lunch received duplicate --host".to_string());
                }
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| "lunch --host requires a value".to_string())?;
                host = Some(value.clone());
            }
            _ => {
                return Err(
                    "lunch positional arguments are no longer supported; use `lunch --vendor <id> --host <id>` or run `lunch` interactively".to_string(),
                );
            }
        }
        i += 1;
    }

    match (vendor, host) {
        (Some(vendor), Some(host)) => Ok(LunchRequest::Explicit { vendor, host }),
        _ => Err("lunch requires both --vendor and --host for non-interactive use".to_string()),
    }
}

fn choose_lunch_interactively(
    build_config: &BuildConfig,
    vendor_config: &VendorConfig,
    host_config: &HostConfig,
) -> Result<(String, String), String> {
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return Err(
            "interactive lunch requires a terminal; use `lunch --vendor <id> --host <id>` in scripts"
                .to_string(),
        );
    }

    let theme = ColorfulTheme::default();
    let vendor_items = vendor_config
        .vendors
        .iter()
        .map(format_vendor_choice)
        .collect::<Vec<String>>();
    let default_vendor = vendor_config
        .vendors
        .iter()
        .position(|v| v.id == build_config.defaults.vendor)
        .unwrap_or(0);
    let vendor_index = Select::with_theme(&theme)
        .with_prompt("Select vendor build scope")
        .items(&vendor_items)
        .default(default_vendor)
        .interact_opt()
        .map_err(|e| format!("interactive vendor selection failed: {e}"))?
        .ok_or_else(|| "lunch cancelled".to_string())?;
    let vendor = &vendor_config.vendors[vendor_index];

    let host_choices = allowed_hosts_for_vendor(vendor, host_config);
    if host_choices.is_empty() {
        return Err(format!("vendor {} has no allowed host modes", vendor.id));
    }
    let host_items = host_choices
        .iter()
        .map(|h| format!("{} ({})", h.label, h.id))
        .collect::<Vec<String>>();
    let default_host = host_choices
        .iter()
        .position(|h| h.id == build_config.defaults.host)
        .unwrap_or(0);
    let host_index = Select::with_theme(&theme)
        .with_prompt("Select host mode")
        .items(&host_items)
        .default(default_host)
        .interact_opt()
        .map_err(|e| format!("interactive host selection failed: {e}"))?
        .ok_or_else(|| "lunch cancelled".to_string())?;
    let host = host_choices[host_index];

    Ok((vendor.id.clone(), host.id.clone()))
}

fn format_vendor_choice(vendor: &VendorDef) -> String {
    match vendor.kind.as_str() {
        VENDOR_KIND_MODEL if vendor.implemented => format!("{} ({})", vendor.label, vendor.id),
        VENDOR_KIND_MODEL => format!("{} ({}, placeholder)", vendor.label, vendor.id),
        _ => format!("{} ({})", vendor.label, vendor.id),
    }
}

fn allowed_hosts_for_vendor<'a>(
    vendor: &VendorDef,
    host_config: &'a HostConfig,
) -> Vec<&'a HostDef> {
    host_config
        .hosts
        .iter()
        .filter(|host| {
            host.allowed_vendor_kinds
                .iter()
                .any(|kind| kind == &vendor.kind)
        })
        .collect()
}

fn cmd_env(
    build_config: &BuildConfig,
    vendor_config: &VendorConfig,
    host_config: &HostConfig,
    selected_path: &Path,
    args: &[String],
) -> Result<(), String> {
    if args.len() != 2 || args[0] != "--shell" {
        return Err("env requires --shell bash|zsh".to_string());
    }
    let shell = args[1].as_str();
    if shell != "bash" && shell != "zsh" {
        return Err("env --shell supports only bash|zsh".to_string());
    }

    let resolved = resolve_selected(build_config, vendor_config, host_config, selected_path)?;
    println!("export WIREMUX_VENDOR={}", shell_quote(&resolved.vendor));
    println!(
        "export WIREMUX_VENDOR_KIND={}",
        shell_quote(&resolved.vendor_kind)
    );
    println!("export WIREMUX_HOST={}", shell_quote(&resolved.host));
    println!(
        "export WIREMUX_HOST_PROFILE={}",
        shell_quote(&resolved.host_profile)
    );
    if let Some(family) = resolved.vendor_family {
        println!("export WIREMUX_VENDOR_FAMILY={}", shell_quote(&family));
    }
    if let Some(idf_target) = resolved.vendor_idf_target {
        println!("export WIREMUX_IDF_TARGET={}", shell_quote(&idf_target));
    }
    if let Some(example_path) = resolved.vendor_example_path {
        println!(
            "export WIREMUX_VENDOR_EXAMPLE={}",
            shell_quote(&example_path)
        );
    }
    Ok(())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn cmd_doctor(repo_root: &Path, config: &BuildConfig, metadata_path: &Path) -> Result<(), String> {
    let ci = is_ci();
    let dirty = git_dirty(repo_root).unwrap_or(false);

    check_tool_contract("python3", &["--version"], config.tools.get("python"), ci)?;
    check_tool_contract("rustc", &["--version"], config.tools.get("rustc"), ci)?;
    check_tool_contract("cargo", &["--version"], config.tools.get("cargo"), ci)?;
    check_tool_contract("cmake", &["--version"], config.tools.get("cmake"), ci)?;
    check_tool_contract("ctest", &["--version"], config.tools.get("ctest"), ci)?;

    let idf_ok =
        check_optional_tool_contract("idf.py", &["--version"], config.tools.get("idf_py"), ci)?;

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
    build_config: &BuildConfig,
    vendor_config: &VendorConfig,
    host_config: &HostConfig,
    selected_path: &Path,
    metadata_path: &Path,
    args: &[String],
) -> Result<(), String> {
    let ci = is_ci();
    if args.len() != 1 {
        return Err("check requires core|host|vendor|vendor-espressif|all".to_string());
    }
    match args[0].as_str() {
        "core" => run_core_check(repo_root)?,
        "host" => {
            let selected =
                resolve_selected(build_config, vendor_config, host_config, selected_path)?;
            run_host_check(repo_root, vendor_config, &selected)?;
        }
        "vendor" | "vendor-espressif" => {
            let selected =
                resolve_selected(build_config, vendor_config, host_config, selected_path)?;
            run_vendor_build(
                repo_root,
                vendor_config,
                &selected,
                build_config.tools.get("idf_py"),
                ci,
                true,
            )?;
        }
        "all" => {
            let selected =
                resolve_selected(build_config, vendor_config, host_config, selected_path)?;
            run_core_check(repo_root)?;
            run_host_check(repo_root, vendor_config, &selected)?;
            run_vendor_build(
                repo_root,
                vendor_config,
                &selected,
                build_config.tools.get("idf_py"),
                ci,
                true,
            )?;
        }
        _ => return Err("check requires core|host|vendor|vendor-espressif|all".to_string()),
    }
    append_metadata(
        metadata_path,
        &MetadataRecord {
            event: "check".to_string(),
            unix: now_unix(),
            target: Some(args[0].clone()),
            status: Some("ok".to_string()),
            ci: None,
            dirty: None,
            idf_py: None,
        },
    )?;
    Ok(())
}

fn cmd_build(
    repo_root: &Path,
    build_config: &BuildConfig,
    vendor_config: &VendorConfig,
    host_config: &HostConfig,
    selected_path: &Path,
    metadata_path: &Path,
    args: &[String],
) -> Result<(), String> {
    let ci = is_ci();
    if args.len() != 1 {
        return Err("build requires core|host|vendor|vendor-espressif".to_string());
    }
    match args[0].as_str() {
        "core" => run_core_build(repo_root)?,
        "host" => {
            let selected =
                resolve_selected(build_config, vendor_config, host_config, selected_path)?;
            run_host_build(repo_root, vendor_config, &selected)?;
        }
        "vendor" | "vendor-espressif" => {
            let selected =
                resolve_selected(build_config, vendor_config, host_config, selected_path)?;
            run_vendor_build(
                repo_root,
                vendor_config,
                &selected,
                build_config.tools.get("idf_py"),
                ci,
                false,
            )?;
        }
        _ => return Err("build requires core|host|vendor|vendor-espressif".to_string()),
    }
    append_metadata(
        metadata_path,
        &MetadataRecord {
            event: "build".to_string(),
            unix: now_unix(),
            target: Some(args[0].clone()),
            status: Some("ok".to_string()),
            ci: None,
            dirty: None,
            idf_py: None,
        },
    )?;
    Ok(())
}

fn cmd_package(
    repo_root: &Path,
    _config: &BuildConfig,
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
    append_metadata(
        metadata_path,
        &MetadataRecord {
            event: "package".to_string(),
            unix: now_unix(),
            target: Some("esp-registry".to_string()),
            status: Some("ok".to_string()),
            ci: None,
            dirty: None,
            idf_py: None,
        },
    )?;
    Ok(())
}

fn resolve_selected(
    build_config: &BuildConfig,
    vendor_config: &VendorConfig,
    host_config: &HostConfig,
    selected_path: &Path,
) -> Result<Selected, String> {
    if let Some(selected) = read_selected(selected_path)? {
        return resolve_selection(
            vendor_config,
            host_config,
            &selected.vendor,
            &selected.host,
            selected.selected_at_unix,
        );
    }

    resolve_selection(
        vendor_config,
        host_config,
        &build_config.defaults.vendor,
        &build_config.defaults.host,
        None,
    )
}

fn resolve_selection(
    vendor_config: &VendorConfig,
    host_config: &HostConfig,
    vendor_id: &str,
    host_id: &str,
    selected_at_unix: Option<u64>,
) -> Result<Selected, String> {
    let (vendor, host) = validate_selection(vendor_config, host_config, vendor_id, host_id)?;
    Ok(selected_from_defs(vendor, host, selected_at_unix))
}

fn validate_selection<'a>(
    vendor_config: &'a VendorConfig,
    host_config: &'a HostConfig,
    vendor_id: &str,
    host_id: &str,
) -> Result<(&'a VendorDef, &'a HostDef), String> {
    let vendor = find_vendor(vendor_config, vendor_id)?;
    let host = find_host(host_config, host_id)?;
    if !host
        .allowed_vendor_kinds
        .iter()
        .any(|kind| kind == &vendor.kind)
    {
        return Err(format!(
            "host mode {} is not valid for vendor {} ({})",
            host.id, vendor.id, vendor.kind
        ));
    }
    Ok((vendor, host))
}

fn selected_from_defs(
    vendor: &VendorDef,
    host: &HostDef,
    selected_at_unix: Option<u64>,
) -> Selected {
    Selected {
        vendor: vendor.id.clone(),
        host: host.id.clone(),
        vendor_kind: vendor.kind.clone(),
        vendor_label: vendor.label.clone(),
        host_profile: host.profile.clone(),
        vendor_family: vendor.family.clone(),
        vendor_idf_target: vendor.idf_target.clone(),
        vendor_example_path: vendor.example_path.clone(),
        selected_at_unix,
    }
}

fn find_vendor<'a>(config: &'a VendorConfig, id: &str) -> Result<&'a VendorDef, String> {
    config
        .vendors
        .iter()
        .find(|vendor| vendor.id == id)
        .ok_or_else(|| format!("unknown vendor: {id}"))
}

fn find_host<'a>(config: &'a HostConfig, id: &str) -> Result<&'a HostDef, String> {
    config
        .hosts
        .iter()
        .find(|host| host.id == id)
        .ok_or_else(|| format!("unknown host mode: {id}"))
}

fn run_core_check(repo_root: &Path) -> Result<(), String> {
    run_native(
        repo_root,
        "cmake",
        &["-S", "sources/core/c", "-B", "sources/core/c/build"],
        None,
    )?;
    run_native(
        repo_root,
        "cmake",
        &["--build", "sources/core/c/build"],
        None,
    )?;
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
    run_native(
        repo_root,
        "cmake",
        &["--build", "sources/core/c/build"],
        None,
    )?;
    Ok(())
}

fn run_host_check(
    repo_root: &Path,
    vendor_config: &VendorConfig,
    selected: &Selected,
) -> Result<(), String> {
    let host_dir = repo_root.join("sources/host/wiremux");
    run_native(&host_dir, "cargo", &["fmt", "--check"], None)?;
    run_native_owned(
        &host_dir,
        "cargo",
        &cargo_args_with_host_features("check", vendor_config, selected)?,
        None,
    )?;
    run_native_owned(
        &host_dir,
        "cargo",
        &cargo_args_with_host_features("test", vendor_config, selected)?,
        None,
    )?;
    Ok(())
}

fn run_host_build(
    repo_root: &Path,
    vendor_config: &VendorConfig,
    selected: &Selected,
) -> Result<(), String> {
    let host_dir = repo_root.join("sources/host/wiremux");
    run_native_owned(
        &host_dir,
        "cargo",
        &cargo_args_with_host_features("build", vendor_config, selected)?,
        None,
    )?;
    Ok(())
}

fn cargo_args_with_host_features(
    command: &str,
    vendor_config: &VendorConfig,
    selected: &Selected,
) -> Result<Vec<String>, String> {
    let feature = match selected.host.as_str() {
        HOST_GENERIC => "generic".to_string(),
        HOST_ALL_FEATURES => HOST_ALL_FEATURES.to_string(),
        HOST_VENDOR_ENHANCED => {
            if selected.vendor_kind != VENDOR_KIND_MODEL {
                return Err("vendor-enhanced host mode requires a single vendor model".to_string());
            }
            let vendor = find_vendor(vendor_config, &selected.vendor)?;
            if !vendor.implemented {
                return Err(format!(
                    "vendor target {} is listed but host dispatch is not implemented yet",
                    vendor.id
                ));
            }
            vendor.host_feature.clone().ok_or_else(|| {
                format!(
                    "vendor target {} does not define a host feature for vendor-enhanced mode",
                    vendor.id
                )
            })?
        }
        _ => return Err(format!("unknown selected host mode: {}", selected.host)),
    };

    Ok(vec![command.to_string(), "--features".to_string(), feature])
}

fn run_vendor_build(
    repo_root: &Path,
    vendor_config: &VendorConfig,
    selected: &Selected,
    idf_contract: Option<&ToolContract>,
    ci: bool,
    local_skip_ok: bool,
) -> Result<(), String> {
    let vendors = selected_vendor_targets(vendor_config, selected)?;
    if vendors.is_empty() {
        println!("skip: vendor scope is skip; vendor build skipped");
        return Ok(());
    }

    let idf_ok = check_optional_tool_contract("idf.py", &["--version"], idf_contract, ci)?;
    if !idf_ok {
        if local_skip_ok && !ci {
            println!("skip: idf.py not found; vendor check skipped for local environment");
            return Ok(());
        }
        return Err("idf.py not found; cannot build selected vendor targets".to_string());
    }

    for vendor in vendors {
        run_vendor_model(repo_root, vendor)?;
    }
    Ok(())
}

fn selected_vendor_targets<'a>(
    vendor_config: &'a VendorConfig,
    selected: &Selected,
) -> Result<Vec<&'a VendorDef>, String> {
    match selected.vendor_kind.as_str() {
        VENDOR_KIND_SKIP => Ok(Vec::new()),
        VENDOR_KIND_MODEL => {
            let vendor = find_vendor(vendor_config, &selected.vendor)?;
            if !vendor.implemented {
                return Err(format!(
                    "vendor target {} is listed but build dispatch is not implemented yet",
                    vendor.id
                ));
            }
            Ok(vec![vendor])
        }
        VENDOR_KIND_ALL => {
            let vendors = vendor_config
                .vendors
                .iter()
                .filter(|vendor| {
                    vendor.kind == VENDOR_KIND_MODEL && vendor.implemented && vendor.include_in_all
                })
                .collect::<Vec<&VendorDef>>();
            if vendors.is_empty() {
                return Err("vendor scope all has no implemented vendor targets".to_string());
            }
            Ok(vendors)
        }
        _ => Err(format!(
            "unknown selected vendor kind: {}",
            selected.vendor_kind
        )),
    }
}

fn run_vendor_model(repo_root: &Path, vendor: &VendorDef) -> Result<(), String> {
    let family = vendor
        .family
        .as_deref()
        .ok_or_else(|| format!("vendor target {} does not define a family", vendor.id))?;
    if family != "espressif" {
        return Err(format!(
            "vendor family {family} for {} is listed but build dispatch is not implemented yet",
            vendor.id
        ));
    }
    let idf_target = vendor
        .idf_target
        .as_deref()
        .ok_or_else(|| format!("vendor target {} does not define idf_target", vendor.id))?;
    let example_path = vendor
        .example_path
        .as_deref()
        .ok_or_else(|| format!("vendor target {} does not define example_path", vendor.id))?;
    let vendor_dir = repo_root.join(example_path);
    run_native(&vendor_dir, "idf.py", &["set-target", idf_target], None)?;
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

fn run_native(
    cwd: &Path,
    program: &str,
    args: &[&str],
    extra_env: Option<(&str, &str)>,
) -> Result<(), String> {
    let owned_args = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<String>>();
    run_native_owned(cwd, program, &owned_args, extra_env)
}

fn run_native_owned(
    cwd: &Path,
    program: &str,
    args: &[String],
    extra_env: Option<(&str, &str)>,
) -> Result<(), String> {
    let cmdline = std::iter::once(program.to_string())
        .chain(args.iter().cloned())
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
    env::var("CI")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_vendors() -> VendorConfig {
        VendorConfig {
            vendors: vec![
                VendorDef {
                    id: "skip".to_string(),
                    label: "Skip vendor builds".to_string(),
                    kind: VENDOR_KIND_SKIP.to_string(),
                    family: None,
                    idf_target: None,
                    example_path: None,
                    host_feature: None,
                    implemented: false,
                    include_in_all: false,
                },
                VendorDef {
                    id: "all".to_string(),
                    label: "All implemented vendor targets".to_string(),
                    kind: VENDOR_KIND_ALL.to_string(),
                    family: None,
                    idf_target: None,
                    example_path: None,
                    host_feature: None,
                    implemented: false,
                    include_in_all: false,
                },
                VendorDef {
                    id: "esp32-s3".to_string(),
                    label: "Espressif ESP32-S3".to_string(),
                    kind: VENDOR_KIND_MODEL.to_string(),
                    family: Some("espressif".to_string()),
                    idf_target: Some("esp32s3".to_string()),
                    example_path: Some(
                        "sources/vendor/espressif/generic/examples/esp_wiremux_console_demo"
                            .to_string(),
                    ),
                    host_feature: Some("esp32".to_string()),
                    implemented: true,
                    include_in_all: true,
                },
                VendorDef {
                    id: "esp32-p4".to_string(),
                    label: "Espressif ESP32-P4".to_string(),
                    kind: VENDOR_KIND_MODEL.to_string(),
                    family: Some("espressif".to_string()),
                    idf_target: Some("esp32p4".to_string()),
                    example_path: Some(
                        "sources/vendor/espressif/generic/examples/esp_wiremux_console_demo"
                            .to_string(),
                    ),
                    host_feature: Some("esp32".to_string()),
                    implemented: false,
                    include_in_all: false,
                },
            ],
        }
    }

    fn sample_hosts() -> HostConfig {
        HostConfig {
            hosts: vec![
                HostDef {
                    id: HOST_GENERIC.to_string(),
                    label: "Generic host".to_string(),
                    profile: "generic".to_string(),
                    allowed_vendor_kinds: vec![
                        VENDOR_KIND_SKIP.to_string(),
                        VENDOR_KIND_ALL.to_string(),
                        VENDOR_KIND_MODEL.to_string(),
                    ],
                },
                HostDef {
                    id: HOST_VENDOR_ENHANCED.to_string(),
                    label: "Vendor enhanced".to_string(),
                    profile: "vendor-enhanced".to_string(),
                    allowed_vendor_kinds: vec![VENDOR_KIND_MODEL.to_string()],
                },
                HostDef {
                    id: HOST_ALL_FEATURES.to_string(),
                    label: "All features".to_string(),
                    profile: "all-features".to_string(),
                    allowed_vendor_kinds: vec![
                        VENDOR_KIND_SKIP.to_string(),
                        VENDOR_KIND_ALL.to_string(),
                        VENDOR_KIND_MODEL.to_string(),
                    ],
                },
            ],
        }
    }

    #[test]
    fn lunch_flags_require_vendor_and_host() {
        let args = vec![
            "--vendor".to_string(),
            "esp32-s3".to_string(),
            "--host".to_string(),
            HOST_VENDOR_ENHANCED.to_string(),
        ];
        let parsed = parse_lunch_args(&args).unwrap();
        match parsed {
            LunchRequest::Explicit { vendor, host } => {
                assert_eq!(vendor, "esp32-s3");
                assert_eq!(host, HOST_VENDOR_ENHANCED);
            }
            LunchRequest::Interactive => panic!("expected explicit lunch request"),
        }
    }

    #[test]
    fn lunch_rejects_positional_arguments() {
        let args = vec!["esp32-s3".to_string(), HOST_VENDOR_ENHANCED.to_string()];
        let err = parse_lunch_args(&args).unwrap_err();
        assert!(err.contains("positional arguments are no longer supported"));
    }

    #[test]
    fn vendor_enhanced_requires_model_vendor() {
        let vendors = sample_vendors();
        let hosts = sample_hosts();
        let err = validate_selection(&vendors, &hosts, "skip", HOST_VENDOR_ENHANCED).unwrap_err();
        assert!(err.contains("not valid"));
        validate_selection(&vendors, &hosts, "esp32-s3", HOST_VENDOR_ENHANCED).unwrap();
    }

    #[test]
    fn vendor_all_dispatches_only_implemented_included_models() {
        let vendors = sample_vendors();
        let selected =
            resolve_selection(&vendors, &sample_hosts(), "all", HOST_GENERIC, None).unwrap();
        let targets = selected_vendor_targets(&vendors, &selected).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].id, "esp32-s3");
    }

    #[test]
    fn placeholder_model_fails_for_vendor_dispatch() {
        let vendors = sample_vendors();
        let selected =
            resolve_selection(&vendors, &sample_hosts(), "esp32-p4", HOST_GENERIC, None).unwrap();
        let err = selected_vendor_targets(&vendors, &selected).unwrap_err();
        assert!(err.contains("not implemented yet"));
    }
}
