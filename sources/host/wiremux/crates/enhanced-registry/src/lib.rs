use std::collections::HashMap;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiStability {
    Development,
    Stable,
    Frozen,
}

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
pub struct CapabilityRequirement {
    id: CapabilityId,
    description: String,
}

impl CapabilityRequirement {
    pub fn new(id: CapabilityId, description: impl Into<String>) -> Self {
        Self {
            id,
            description: description.into(),
        }
    }

    pub fn id(&self) -> &CapabilityId {
        &self.id
    }

    pub fn description(&self) -> &str {
        &self.description
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityDeclaration {
    id: CapabilityId,
    stability: ApiStability,
    description: String,
    requirements: Vec<CapabilityRequirement>,
}

impl CapabilityDeclaration {
    pub fn new(
        id: CapabilityId,
        stability: ApiStability,
        description: impl Into<String>,
        requirements: Vec<CapabilityRequirement>,
    ) -> Self {
        Self {
            id,
            stability,
            description: description.into(),
            requirements,
        }
    }

    pub fn id(&self) -> &CapabilityId {
        &self.id
    }

    pub fn stability(&self) -> ApiStability {
        self.stability
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn requirements(&self) -> &[CapabilityRequirement] {
        &self.requirements
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityCatalog {
    current_version: u32,
    capabilities: Vec<CapabilityDeclaration>,
}

impl CapabilityCatalog {
    pub fn new(current_version: u32, capabilities: Vec<CapabilityDeclaration>) -> Self {
        Self {
            current_version,
            capabilities,
        }
    }

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
    DuplicateProvider(CapabilityId),
}

impl fmt::Display for RegistryError {
    fn fmt(&self, frame: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateProvider(id) => {
                write!(frame, "enhanced provider already registered for {id}")
            }
        }
    }
}

impl std::error::Error for RegistryError {}

#[derive(Debug, PartialEq, Eq)]
pub enum ResolveError {
    ProviderNotRegistered(CapabilityId),
}

impl fmt::Display for ResolveError {
    fn fmt(&self, frame: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProviderNotRegistered(id) => {
                write!(frame, "enhanced provider is not registered for {id}")
            }
        }
    }
}

impl std::error::Error for ResolveError {}

#[derive(Clone, Debug, Default)]
pub struct ProviderRegistry {
    providers: HashMap<CapabilityId, ProviderRegistration>,
}

impl ProviderRegistry {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_resolves_registered_provider() {
        let capability = CapabilityDeclaration::new(
            CapabilityId::new("wiremux.test.capability", 1),
            ApiStability::Frozen,
            "test capability",
            Vec::new(),
        );
        let id = capability.id().clone();
        let mut registry = ProviderRegistry::new();

        registry
            .register(capability, "test_provider")
            .expect("provider registers");

        let provider = registry.resolve(&id).expect("provider resolves");
        assert_eq!(provider.provider_key(), "test_provider");
        assert!(registry.supports(&id));
    }

    #[test]
    fn registry_rejects_duplicate_provider() {
        let capability = CapabilityDeclaration::new(
            CapabilityId::new("wiremux.test.capability", 1),
            ApiStability::Frozen,
            "test capability",
            Vec::new(),
        );
        let mut registry = ProviderRegistry::new();
        registry
            .register(capability.clone(), "first")
            .expect("provider registers");

        let err = registry
            .register(capability, "second")
            .expect_err("duplicate provider is rejected");

        assert!(matches!(err, RegistryError::DuplicateProvider(_)));
    }

    #[test]
    fn registry_reports_missing_provider() {
        let registry = ProviderRegistry::new();
        let id = CapabilityId::new("wiremux.test.capability", 1);

        let err = registry
            .resolve(&id)
            .expect_err("missing provider is reported");

        assert_eq!(err, ResolveError::ProviderNotRegistered(id));
    }
}
