use std::collections::HashMap;
use std::fmt;

use prost::Message;

pub mod proto {
    include!(concat!(
        env!("OUT_DIR"),
        "/wiremux.host.generic_enhanced.v1.rs"
    ));
}

const LATEST_CATALOG_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/generic_enhanced_catalog.pb"));
const GENERIC_ENHANCED_API_PREFIX: &str = "wiremux.generic.enhanced.";
const VIRTUAL_SERIAL_PROVIDER_KEY: &str = "virtual_serial";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CapabilityId {
    api_name: String,
    frozen_version: u32,
}

impl CapabilityId {
    pub fn new(api_name: impl Into<String>, frozen_version: u32) -> Self {
        Self {
            api_name: api_name.into(),
            frozen_version,
        }
    }

    pub fn api_name(&self) -> &str {
        &self.api_name
    }

    pub fn frozen_version(&self) -> u32 {
        self.frozen_version
    }
}

impl fmt::Display for CapabilityId {
    fn fmt(&self, frame: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(frame, "{}@{}", self.api_name, self.frozen_version)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityDeclaration {
    id: CapabilityId,
    stability: proto::GenericEnhancedApiStability,
    description: String,
}

impl CapabilityDeclaration {
    pub fn id(&self) -> &CapabilityId {
        &self.id
    }

    pub fn stability(&self) -> proto::GenericEnhancedApiStability {
        self.stability
    }

    pub fn description(&self) -> &str {
        &self.description
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityCatalog {
    current_version: u32,
    capabilities: Vec<CapabilityDeclaration>,
}

impl CapabilityCatalog {
    pub fn current_version(&self) -> u32 {
        self.current_version
    }

    pub fn capabilities(&self) -> &[CapabilityDeclaration] {
        &self.capabilities
    }

    pub fn find(&self, id: &CapabilityId) -> Option<&CapabilityDeclaration> {
        self.capabilities
            .iter()
            .find(|capability| capability.id == *id)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum CatalogError {
    Decode(String),
    EmptyApiName,
    MissingFrozenVersion { api_name: String },
    UnknownStability { api_name: String, value: i32 },
    DuplicateCapability(CapabilityId),
    VirtualSerialMissing,
    VirtualSerialAmbiguous,
}

impl fmt::Display for CatalogError {
    fn fmt(&self, frame: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(err) => write!(frame, "failed to decode generic enhanced catalog: {err}"),
            Self::EmptyApiName => {
                frame.write_str("generic enhanced catalog contains an empty api_name")
            }
            Self::MissingFrozenVersion { api_name } => {
                write!(
                    frame,
                    "generic enhanced catalog entry {api_name} has frozen_version = 0"
                )
            }
            Self::UnknownStability { api_name, value } => {
                write!(
                    frame,
                    "generic enhanced catalog entry {api_name} has unknown stability {value}"
                )
            }
            Self::DuplicateCapability(id) => {
                write!(
                    frame,
                    "generic enhanced catalog contains duplicate capability {id}"
                )
            }
            Self::VirtualSerialMissing => {
                frame.write_str("generic enhanced catalog does not declare virtual serial")
            }
            Self::VirtualSerialAmbiguous => frame.write_str(
                "generic enhanced catalog declares multiple virtual serial capabilities",
            ),
        }
    }
}

impl std::error::Error for CatalogError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderRegistration {
    capability: CapabilityDeclaration,
    provider_key: String,
}

impl ProviderRegistration {
    pub fn capability(&self) -> &CapabilityDeclaration {
        &self.capability
    }

    pub fn provider_key(&self) -> &str {
        &self.provider_key
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RegistryError {
    Catalog(CatalogError),
    DuplicateProvider(CapabilityId),
}

impl fmt::Display for RegistryError {
    fn fmt(&self, frame: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(err) => err.fmt(frame),
            Self::DuplicateProvider(id) => {
                write!(
                    frame,
                    "generic enhanced provider already registered for {id}"
                )
            }
        }
    }
}

impl std::error::Error for RegistryError {}

impl From<CatalogError> for RegistryError {
    fn from(err: CatalogError) -> Self {
        Self::Catalog(err)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ResolveError {
    ProviderNotRegistered(CapabilityId),
}

impl fmt::Display for ResolveError {
    fn fmt(&self, frame: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProviderNotRegistered(id) => {
                write!(
                    frame,
                    "generic enhanced provider is not registered for {id}"
                )
            }
        }
    }
}

impl std::error::Error for ResolveError {}

#[derive(Clone, Debug, Default)]
pub struct GenericEnhancedRegistry {
    providers: HashMap<CapabilityId, ProviderRegistration>,
}

impl GenericEnhancedRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        capability: CapabilityDeclaration,
        provider_key: impl Into<String>,
    ) -> Result<(), RegistryError> {
        let id = capability.id.clone();
        if self.providers.contains_key(&id) {
            return Err(RegistryError::DuplicateProvider(id));
        }
        self.providers.insert(
            id,
            ProviderRegistration {
                capability,
                provider_key: provider_key.into(),
            },
        );
        Ok(())
    }

    pub fn resolve(&self, id: &CapabilityId) -> Result<&ProviderRegistration, ResolveError> {
        self.providers
            .get(id)
            .ok_or_else(|| ResolveError::ProviderNotRegistered(id.clone()))
    }

    pub fn supports(&self, id: &CapabilityId) -> bool {
        self.providers.contains_key(id)
    }
}

pub fn latest_catalog() -> Result<CapabilityCatalog, CatalogError> {
    decode_catalog(LATEST_CATALOG_BYTES)
}

pub fn latest_virtual_serial_declaration() -> Result<CapabilityDeclaration, CatalogError> {
    let catalog = latest_catalog()?;
    virtual_serial_declaration(&catalog).cloned()
}

pub fn latest_virtual_serial_capability_id() -> Result<CapabilityId, CatalogError> {
    latest_virtual_serial_declaration().map(|declaration| declaration.id)
}

pub fn register_virtual_serial_provider(
    registry: &mut GenericEnhancedRegistry,
) -> Result<(), RegistryError> {
    registry.register(
        latest_virtual_serial_declaration()?,
        VIRTUAL_SERIAL_PROVIDER_KEY,
    )
}

fn decode_catalog(bytes: &[u8]) -> Result<CapabilityCatalog, CatalogError> {
    let raw = proto::GenericEnhancedApiCatalog::decode(bytes)
        .map_err(|err| CatalogError::Decode(err.to_string()))?;
    convert_catalog(raw)
}

fn convert_catalog(
    raw: proto::GenericEnhancedApiCatalog,
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
        let stability =
            proto::GenericEnhancedApiStability::try_from(api.stability).map_err(|_| {
                CatalogError::UnknownStability {
                    api_name: api_name.clone(),
                    value: api.stability,
                }
            })?;
        let id = CapabilityId::new(api_name, api.frozen_version);
        if capabilities
            .iter()
            .any(|capability: &CapabilityDeclaration| capability.id == id)
        {
            return Err(CatalogError::DuplicateCapability(id));
        }
        capabilities.push(CapabilityDeclaration {
            id,
            stability,
            description: api.description,
        });
    }
    Ok(CapabilityCatalog {
        current_version: raw.current_version,
        capabilities,
    })
}

fn virtual_serial_declaration(
    catalog: &CapabilityCatalog,
) -> Result<&CapabilityDeclaration, CatalogError> {
    let mut matches = catalog.capabilities.iter().filter(|capability| {
        capability
            .id
            .api_name
            .strip_prefix(GENERIC_ENHANCED_API_PREFIX)
            == Some(VIRTUAL_SERIAL_PROVIDER_KEY)
    });
    let Some(first) = matches.next() else {
        return Err(CatalogError::VirtualSerialMissing);
    };
    if matches.next().is_some() {
        return Err(CatalogError::VirtualSerialAmbiguous);
    }
    Ok(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_catalog_decodes_virtual_serial_capability() {
        let catalog = latest_catalog().expect("catalog decodes");

        assert_eq!(catalog.current_version(), 1);
        let declaration =
            latest_virtual_serial_declaration().expect("virtual serial declaration exists");
        assert_eq!(
            declaration.id().api_name(),
            "wiremux.generic.enhanced.virtual_serial"
        );
        assert_eq!(declaration.id().frozen_version(), 1);
        assert_eq!(
            declaration.stability(),
            proto::GenericEnhancedApiStability::Frozen
        );
        assert!(catalog.find(declaration.id()).is_some());
    }

    #[test]
    fn registry_resolves_registered_virtual_serial_provider() {
        let mut registry = GenericEnhancedRegistry::new();
        register_virtual_serial_provider(&mut registry).expect("provider registers");
        let id = latest_virtual_serial_capability_id().expect("capability id exists");

        let provider = registry.resolve(&id).expect("provider resolves");

        assert_eq!(provider.provider_key(), VIRTUAL_SERIAL_PROVIDER_KEY);
        assert!(registry.supports(&id));
    }

    #[test]
    fn registry_rejects_duplicate_provider() {
        let mut registry = GenericEnhancedRegistry::new();
        register_virtual_serial_provider(&mut registry).expect("provider registers");

        let err = register_virtual_serial_provider(&mut registry)
            .expect_err("duplicate provider is rejected");

        assert!(matches!(err, RegistryError::DuplicateProvider(_)));
    }

    #[test]
    fn registry_reports_missing_provider() {
        let registry = GenericEnhancedRegistry::new();
        let id = latest_virtual_serial_capability_id().expect("capability id exists");

        let err = registry
            .resolve(&id)
            .expect_err("missing provider is reported");

        assert_eq!(err, ResolveError::ProviderNotRegistered(id));
    }

    #[test]
    fn catalog_rejects_duplicate_capability() {
        let raw = proto::GenericEnhancedApiCatalog {
            current_version: 1,
            apis: vec![
                proto::GenericEnhancedApiDeclaration {
                    api_name: "wiremux.generic.enhanced.virtual_serial".to_string(),
                    frozen_version: 1,
                    stability: proto::GenericEnhancedApiStability::Frozen as i32,
                    description: String::new(),
                    typed_config: None,
                },
                proto::GenericEnhancedApiDeclaration {
                    api_name: "wiremux.generic.enhanced.virtual_serial".to_string(),
                    frozen_version: 1,
                    stability: proto::GenericEnhancedApiStability::Frozen as i32,
                    description: String::new(),
                    typed_config: None,
                },
            ],
        };

        let err = convert_catalog(raw).expect_err("duplicate capability is rejected");

        assert!(matches!(err, CatalogError::DuplicateCapability(_)));
    }

    #[test]
    fn virtual_serial_lookup_uses_generic_enhanced_api_namespace() {
        let raw = proto::GenericEnhancedApiCatalog {
            current_version: 1,
            apis: vec![proto::GenericEnhancedApiDeclaration {
                api_name: "wiremux.vendor.enhanced.virtual_serial".to_string(),
                frozen_version: 1,
                stability: proto::GenericEnhancedApiStability::Frozen as i32,
                description: String::new(),
                typed_config: None,
            }],
        };
        let catalog = convert_catalog(raw).expect("catalog converts");

        let err =
            virtual_serial_declaration(&catalog).expect_err("vendor namespace is not generic");

        assert_eq!(err, CatalogError::VirtualSerialMissing);
    }
}
