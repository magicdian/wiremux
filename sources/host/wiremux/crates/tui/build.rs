use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct StatusConfig {
    version: u32,
    navigation: String,
    field: Vec<StatusField>,
}

#[derive(Debug, Deserialize)]
struct StatusField {
    id: String,
    label: String,
    priority: u16,
    summary: bool,
}

fn main() {
    println!("cargo:rerun-if-changed=status-fields.toml");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let config_path = manifest_dir.join("status-fields.toml");
    let config = fs::read_to_string(&config_path).expect("read status-fields.toml");
    let config: StatusConfig = toml::from_str(&config).expect("parse status-fields.toml");
    validate(&config);

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    fs::write(
        out_dir.join("status_fields.rs"),
        generated_status_fields(&config),
    )
    .expect("write generated status fields");
}

fn validate(config: &StatusConfig) {
    assert_eq!(config.version, 1, "unsupported status-fields.toml version");
    assert_eq!(
        config.navigation, "clamp",
        "only clamp status navigation is implemented"
    );
    assert!(
        !config.field.is_empty(),
        "status-fields.toml must define at least one field"
    );

    let mut ids = BTreeSet::new();
    for field in &config.field {
        assert!(
            ids.insert(field.id.as_str()),
            "duplicate status field id: {}",
            field.id
        );
    }
}

fn generated_status_fields(config: &StatusConfig) -> String {
    let mut fields = config.field.iter().collect::<Vec<_>>();
    fields.sort_by_key(|field| (field.priority, field.id.as_str()));

    let mut output = String::from(
        "const STATUS_NAVIGATION_MODE: StatusNavigationMode = StatusNavigationMode::Clamp;\n",
    );
    output.push_str("const STATUS_FIELD_DEFINITIONS: &[StatusFieldDefinition] = &[\n");
    for field in fields {
        output.push_str("    StatusFieldDefinition {\n");
        output.push_str(&format!("        id: {:?},\n", field.id));
        output.push_str(&format!("        label: {:?},\n", field.label));
        output.push_str(&format!("        priority: {},\n", field.priority));
        output.push_str(&format!("        summary: {},\n", field.summary));
        output.push_str("    },\n");
    }
    output.push_str("];\n");
    output
}
