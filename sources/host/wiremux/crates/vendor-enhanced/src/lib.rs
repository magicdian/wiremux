use std::collections::HashSet;
use std::fmt;

use enhanced_registry::{
    ApiStability, CapabilityCatalog, CapabilityDeclaration, CapabilityId, CapabilityRequirement,
    ProviderRegistry, RegistryError,
};
use prost::Message;

pub use enhanced_registry::{ProviderRegistration, ResolveError};

pub mod proto {
    include!(concat!(
        env!("OUT_DIR"),
        "/wiremux.host.vendor_enhanced.espressif.v1.rs"
    ));
}

const LATEST_ESPRESSIF_CATALOG_BYTES: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/espressif_vendor_enhanced_catalog.pb"
));
const ESPRESSIF_VENDOR_ENHANCED_API_PREFIX: &str = "wiremux.vendor.enhanced.espressif.";
const ESPTOOL_BRIDGE_PROVIDER_KEY: &str = "esptool_bridge";
const GENERIC_ENHANCED_VIRTUAL_SERIAL_API: &str = "wiremux.generic.enhanced.virtual_serial";

pub type VendorEnhancedRegistry = ProviderRegistry;

#[derive(Debug, PartialEq, Eq)]
pub enum CatalogError {
    Decode(String),
    EmptyApiName,
    MissingFrozenVersion {
        api_name: String,
    },
    UnknownStability {
        api_name: String,
        value: i32,
    },
    DuplicateCapability(CapabilityId),
    EmptyGenericEnhancedRequirement {
        api_name: String,
    },
    MissingGenericEnhancedRequirementVersion {
        api_name: String,
        required_api_name: String,
    },
    DuplicateGenericEnhancedRequirement {
        api_name: String,
        requirement: CapabilityId,
    },
    EsptoolBridgeMissing,
    EsptoolBridgeAmbiguous,
    EsptoolBridgeMissingVirtualSerialRequirement,
}

impl fmt::Display for CatalogError {
    fn fmt(&self, frame: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(err) => {
                write!(frame, "failed to decode Espressif vendor enhanced catalog: {err}")
            }
            Self::EmptyApiName => {
                frame.write_str("Espressif vendor enhanced catalog contains an empty api_name")
            }
            Self::MissingFrozenVersion { api_name } => {
                write!(
                    frame,
                    "Espressif vendor enhanced catalog entry {api_name} has frozen_version = 0"
                )
            }
            Self::UnknownStability { api_name, value } => {
                write!(
                    frame,
                    "Espressif vendor enhanced catalog entry {api_name} has unknown stability {value}"
                )
            }
            Self::DuplicateCapability(id) => {
                write!(
                    frame,
                    "Espressif vendor enhanced catalog contains duplicate capability {id}"
                )
            }
            Self::EmptyGenericEnhancedRequirement { api_name } => {
                write!(
                    frame,
                    "Espressif vendor enhanced catalog entry {api_name} contains an empty generic enhanced requirement"
                )
            }
            Self::MissingGenericEnhancedRequirementVersion {
                api_name,
                required_api_name,
            } => {
                write!(
                    frame,
                    "Espressif vendor enhanced catalog entry {api_name} requires {required_api_name} with frozen_version = 0"
                )
            }
            Self::DuplicateGenericEnhancedRequirement {
                api_name,
                requirement,
            } => {
                write!(
                    frame,
                    "Espressif vendor enhanced catalog entry {api_name} contains duplicate generic enhanced requirement {requirement}"
                )
            }
            Self::EsptoolBridgeMissing => {
                frame.write_str("Espressif vendor enhanced catalog does not declare esptool bridge")
            }
            Self::EsptoolBridgeAmbiguous => frame.write_str(
                "Espressif vendor enhanced catalog declares multiple esptool bridge capabilities",
            ),
            Self::EsptoolBridgeMissingVirtualSerialRequirement => frame.write_str(
                "Espressif vendor enhanced esptool bridge does not require generic enhanced virtual serial",
            ),
        }
    }
}

impl std::error::Error for CatalogError {}

#[derive(Debug, PartialEq, Eq)]
pub enum BuiltInProviderError {
    Catalog(CatalogError),
    Registry(RegistryError),
}

impl fmt::Display for BuiltInProviderError {
    fn fmt(&self, frame: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(err) => err.fmt(frame),
            Self::Registry(err) => err.fmt(frame),
        }
    }
}

impl std::error::Error for BuiltInProviderError {}

impl From<CatalogError> for BuiltInProviderError {
    fn from(err: CatalogError) -> Self {
        Self::Catalog(err)
    }
}

impl From<RegistryError> for BuiltInProviderError {
    fn from(err: RegistryError) -> Self {
        Self::Registry(err)
    }
}

pub fn latest_espressif_catalog() -> Result<CapabilityCatalog, CatalogError> {
    decode_catalog(LATEST_ESPRESSIF_CATALOG_BYTES)
}

pub fn latest_esptool_bridge_declaration() -> Result<CapabilityDeclaration, CatalogError> {
    let catalog = latest_espressif_catalog()?;
    esptool_bridge_declaration(&catalog).cloned()
}

pub fn latest_esptool_bridge_capability_id() -> Result<CapabilityId, CatalogError> {
    latest_esptool_bridge_declaration().map(|declaration| declaration.id().clone())
}

pub fn register_esptool_bridge_provider(
    registry: &mut VendorEnhancedRegistry,
) -> Result<(), BuiltInProviderError> {
    registry.register(
        latest_esptool_bridge_declaration()?,
        ESPTOOL_BRIDGE_PROVIDER_KEY,
    )?;
    Ok(())
}

pub fn built_in_espressif_registry() -> Result<VendorEnhancedRegistry, BuiltInProviderError> {
    let mut registry = VendorEnhancedRegistry::new();
    register_esptool_bridge_provider(&mut registry)?;
    Ok(registry)
}

pub fn host_supports_esptool_bridge_provider() -> bool {
    let Ok(registry) = built_in_espressif_registry() else {
        return false;
    };
    let Ok(capability_id) = latest_esptool_bridge_capability_id() else {
        return false;
    };
    registry.supports(&capability_id)
}

fn decode_catalog(bytes: &[u8]) -> Result<CapabilityCatalog, CatalogError> {
    let raw = proto::EspressifVendorEnhancedApiCatalog::decode(bytes)
        .map_err(|err| CatalogError::Decode(err.to_string()))?;
    convert_catalog(raw)
}

fn convert_catalog(
    raw: proto::EspressifVendorEnhancedApiCatalog,
) -> Result<CapabilityCatalog, CatalogError> {
    let mut capabilities = Vec::with_capacity(raw.apis.len());
    for api in raw.apis {
        let api_name = api.api_name.trim().to_string();
        if api_name.is_empty() {
            return Err(CatalogError::EmptyApiName);
        }
        if api.frozen_version == 0 {
            return Err(CatalogError::MissingFrozenVersion { api_name });
        }
        let stability = convert_stability(api.stability, &api_name)?;
        let id = CapabilityId::new(api_name.clone(), api.frozen_version);
        if capabilities
            .iter()
            .any(|capability: &CapabilityDeclaration| capability.id() == &id)
        {
            return Err(CatalogError::DuplicateCapability(id));
        }
        capabilities.push(CapabilityDeclaration::new(
            id,
            stability,
            api.description,
            convert_generic_requirements(&api_name, api.generic_enhanced_requirements)?,
        ));
    }
    Ok(CapabilityCatalog::new(raw.current_version, capabilities))
}

fn convert_stability(value: i32, api_name: &str) -> Result<ApiStability, CatalogError> {
    let stability = proto::EspressifVendorEnhancedApiStability::try_from(value).map_err(|_| {
        CatalogError::UnknownStability {
            api_name: api_name.to_string(),
            value,
        }
    })?;
    match stability {
        proto::EspressifVendorEnhancedApiStability::Development => Ok(ApiStability::Development),
        proto::EspressifVendorEnhancedApiStability::Stable => Ok(ApiStability::Stable),
        proto::EspressifVendorEnhancedApiStability::Frozen => Ok(ApiStability::Frozen),
        proto::EspressifVendorEnhancedApiStability::Unspecified => {
            Err(CatalogError::UnknownStability {
                api_name: api_name.to_string(),
                value,
            })
        }
    }
}

fn convert_generic_requirements(
    api_name: &str,
    raw_requirements: Vec<proto::HostCapabilityRequirement>,
) -> Result<Vec<CapabilityRequirement>, CatalogError> {
    let mut seen = HashSet::new();
    let mut requirements = Vec::with_capacity(raw_requirements.len());
    for requirement in raw_requirements {
        let required_api_name = requirement.api_name.trim().to_string();
        if required_api_name.is_empty() {
            return Err(CatalogError::EmptyGenericEnhancedRequirement {
                api_name: api_name.to_string(),
            });
        }
        if requirement.frozen_version == 0 {
            return Err(CatalogError::MissingGenericEnhancedRequirementVersion {
                api_name: api_name.to_string(),
                required_api_name,
            });
        }
        let id = CapabilityId::new(required_api_name, requirement.frozen_version);
        if !seen.insert(id.clone()) {
            return Err(CatalogError::DuplicateGenericEnhancedRequirement {
                api_name: api_name.to_string(),
                requirement: id,
            });
        }
        requirements.push(CapabilityRequirement::new(id, requirement.description));
    }
    Ok(requirements)
}

fn esptool_bridge_declaration(
    catalog: &CapabilityCatalog,
) -> Result<&CapabilityDeclaration, CatalogError> {
    let mut matches = catalog.capabilities().iter().filter(|capability| {
        capability
            .id()
            .api_name()
            .strip_prefix(ESPRESSIF_VENDOR_ENHANCED_API_PREFIX)
            == Some(ESPTOOL_BRIDGE_PROVIDER_KEY)
    });
    let Some(first) = matches.next() else {
        return Err(CatalogError::EsptoolBridgeMissing);
    };
    if matches.next().is_some() {
        return Err(CatalogError::EsptoolBridgeAmbiguous);
    }
    if !first.requirements().iter().any(|requirement| {
        requirement.id().api_name() == GENERIC_ENHANCED_VIRTUAL_SERIAL_API
            && requirement.id().frozen_version() == 1
    }) {
        return Err(CatalogError::EsptoolBridgeMissingVirtualSerialRequirement);
    }
    Ok(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_catalog_decodes_esptool_bridge_capability() {
        let catalog = latest_espressif_catalog().expect("catalog decodes");

        assert_eq!(catalog.current_version(), 1);
        let declaration =
            latest_esptool_bridge_declaration().expect("esptool bridge declaration exists");
        assert_eq!(
            declaration.id().api_name(),
            "wiremux.vendor.enhanced.espressif.esptool_bridge"
        );
        assert_eq!(declaration.id().frozen_version(), 1);
        assert_eq!(declaration.stability(), ApiStability::Frozen);
        assert!(catalog.find(declaration.id()).is_some());
    }

    #[test]
    fn esptool_bridge_declares_generic_virtual_serial_requirement_by_reference() {
        let declaration =
            latest_esptool_bridge_declaration().expect("esptool bridge declaration exists");

        let requirements = declaration.requirements();

        assert_eq!(requirements.len(), 1);
        assert_eq!(
            requirements[0].id().api_name(),
            GENERIC_ENHANCED_VIRTUAL_SERIAL_API
        );
        assert_eq!(requirements[0].id().frozen_version(), 1);
    }

    #[test]
    fn registry_resolves_registered_esptool_bridge_provider() {
        let registry = built_in_espressif_registry().expect("provider registers");
        let id = latest_esptool_bridge_capability_id().expect("capability id exists");

        let provider = registry.resolve(&id).expect("provider resolves");

        assert_eq!(provider.provider_key(), ESPTOOL_BRIDGE_PROVIDER_KEY);
        assert!(registry.supports(&id));
    }

    #[test]
    fn registry_rejects_duplicate_provider() {
        let mut registry = VendorEnhancedRegistry::new();
        register_esptool_bridge_provider(&mut registry).expect("provider registers");

        let err = register_esptool_bridge_provider(&mut registry)
            .expect_err("duplicate provider is rejected");

        assert!(matches!(
            err,
            BuiltInProviderError::Registry(RegistryError::DuplicateProvider(_))
        ));
    }

    #[test]
    fn registry_reports_missing_provider() {
        let registry = VendorEnhancedRegistry::new();
        let id = latest_esptool_bridge_capability_id().expect("capability id exists");

        let err = registry
            .resolve(&id)
            .expect_err("missing provider is reported");

        assert_eq!(err, ResolveError::ProviderNotRegistered(id));
    }

    #[test]
    fn catalog_rejects_duplicate_capability() {
        let raw = proto::EspressifVendorEnhancedApiCatalog {
            current_version: 1,
            apis: vec![
                esptool_bridge_raw_declaration(vec![generic_virtual_serial_requirement()]),
                esptool_bridge_raw_declaration(vec![generic_virtual_serial_requirement()]),
            ],
        };

        let err = convert_catalog(raw).expect_err("duplicate capability is rejected");

        assert!(matches!(err, CatalogError::DuplicateCapability(_)));
    }

    #[test]
    fn catalog_rejects_duplicate_generic_requirement() {
        let raw = proto::EspressifVendorEnhancedApiCatalog {
            current_version: 1,
            apis: vec![esptool_bridge_raw_declaration(vec![
                generic_virtual_serial_requirement(),
                generic_virtual_serial_requirement(),
            ])],
        };

        let err = convert_catalog(raw).expect_err("duplicate requirement is rejected");

        assert!(matches!(
            err,
            CatalogError::DuplicateGenericEnhancedRequirement { .. }
        ));
    }

    #[test]
    fn esptool_bridge_lookup_uses_espressif_vendor_namespace() {
        let raw = proto::EspressifVendorEnhancedApiCatalog {
            current_version: 1,
            apis: vec![proto::EspressifVendorEnhancedApiDeclaration {
                api_name: "wiremux.generic.enhanced.esptool_bridge".to_string(),
                frozen_version: 1,
                stability: proto::EspressifVendorEnhancedApiStability::Frozen as i32,
                description: String::new(),
                generic_enhanced_requirements: vec![generic_virtual_serial_requirement()],
                typed_config: None,
            }],
        };
        let catalog = convert_catalog(raw).expect("catalog converts");

        let err =
            esptool_bridge_declaration(&catalog).expect_err("generic namespace is not vendor");

        assert_eq!(err, CatalogError::EsptoolBridgeMissing);
    }

    #[test]
    fn esptool_bridge_requires_generic_virtual_serial() {
        let raw = proto::EspressifVendorEnhancedApiCatalog {
            current_version: 1,
            apis: vec![esptool_bridge_raw_declaration(Vec::new())],
        };
        let catalog = convert_catalog(raw).expect("catalog converts");

        let err = esptool_bridge_declaration(&catalog)
            .expect_err("missing generic virtual serial requirement is rejected");

        assert_eq!(
            err,
            CatalogError::EsptoolBridgeMissingVirtualSerialRequirement
        );
    }

    fn esptool_bridge_raw_declaration(
        generic_enhanced_requirements: Vec<proto::HostCapabilityRequirement>,
    ) -> proto::EspressifVendorEnhancedApiDeclaration {
        proto::EspressifVendorEnhancedApiDeclaration {
            api_name: "wiremux.vendor.enhanced.espressif.esptool_bridge".to_string(),
            frozen_version: 1,
            stability: proto::EspressifVendorEnhancedApiStability::Frozen as i32,
            description: String::new(),
            generic_enhanced_requirements,
            typed_config: None,
        }
    }

    fn generic_virtual_serial_requirement() -> proto::HostCapabilityRequirement {
        proto::HostCapabilityRequirement {
            api_name: GENERIC_ENHANCED_VIRTUAL_SERIAL_API.to_string(),
            frozen_version: 1,
            description: String::new(),
        }
    }
}
