use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::filter_graph::MediaKind;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceDescriptor {
    pub id: String,
    pub label: String,
    pub media: Vec<MediaKind>,
}

impl ServiceDescriptor {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        media: Vec<MediaKind>,
    ) -> Result<Self, RegistryError> {
        let id = id.into();
        if id.is_empty()
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        {
            return Err(RegistryError::InvalidId(id));
        }
        Ok(Self {
            id,
            label: label.into(),
            media,
        })
    }
}

/// A source port describes media creation; concrete file/process access belongs
/// to an adapter selected after the domain graph has been validated.
pub trait Producer: Send + Sync {
    fn descriptor(&self) -> &ServiceDescriptor;
}

/// A one-input media transformation.
pub trait Filter: Send + Sync {
    fn descriptor(&self) -> &ServiceDescriptor;
}

/// A transformation combining two producer tracks.
pub trait Transition: Send + Sync {
    fn descriptor(&self) -> &ServiceDescriptor;
}

/// A terminal port such as preview, file export, or analysis.
pub trait Consumer: Send + Sync {
    fn descriptor(&self) -> &ServiceDescriptor;
}

#[derive(Default)]
pub struct PipelineRegistry {
    producers: BTreeMap<String, Arc<dyn Producer>>,
    filters: BTreeMap<String, Arc<dyn Filter>>,
    transitions: BTreeMap<String, Arc<dyn Transition>>,
    consumers: BTreeMap<String, Arc<dyn Consumer>>,
}

impl PipelineRegistry {
    pub fn register_producer(&mut self, service: Arc<dyn Producer>) -> Result<(), RegistryError> {
        register(&mut self.producers, service)
    }

    pub fn register_filter(&mut self, service: Arc<dyn Filter>) -> Result<(), RegistryError> {
        register(&mut self.filters, service)
    }

    pub fn register_transition(
        &mut self,
        service: Arc<dyn Transition>,
    ) -> Result<(), RegistryError> {
        register(&mut self.transitions, service)
    }

    pub fn register_consumer(&mut self, service: Arc<dyn Consumer>) -> Result<(), RegistryError> {
        register(&mut self.consumers, service)
    }

    pub fn producer(&self, id: &str) -> Option<Arc<dyn Producer>> {
        self.producers.get(id).cloned()
    }

    pub fn filter(&self, id: &str) -> Option<Arc<dyn Filter>> {
        self.filters.get(id).cloned()
    }

    pub fn transition(&self, id: &str) -> Option<Arc<dyn Transition>> {
        self.transitions.get(id).cloned()
    }

    pub fn consumer(&self, id: &str) -> Option<Arc<dyn Consumer>> {
        self.consumers.get(id).cloned()
    }

    pub fn manifest(&self) -> RegistryManifest {
        RegistryManifest {
            producers: descriptors(&self.producers),
            filters: descriptors(&self.filters),
            transitions: descriptors(&self.transitions),
            consumers: descriptors(&self.consumers),
        }
    }
}

trait Described {
    fn descriptor(&self) -> &ServiceDescriptor;
}

impl Described for dyn Producer {
    fn descriptor(&self) -> &ServiceDescriptor {
        Producer::descriptor(self)
    }
}

impl Described for dyn Filter {
    fn descriptor(&self) -> &ServiceDescriptor {
        Filter::descriptor(self)
    }
}

impl Described for dyn Transition {
    fn descriptor(&self) -> &ServiceDescriptor {
        Transition::descriptor(self)
    }
}

impl Described for dyn Consumer {
    fn descriptor(&self) -> &ServiceDescriptor {
        Consumer::descriptor(self)
    }
}

fn register<T: ?Sized + Described>(
    services: &mut BTreeMap<String, Arc<T>>,
    service: Arc<T>,
) -> Result<(), RegistryError> {
    let id = service.descriptor().id.clone();
    if services.contains_key(&id) {
        return Err(RegistryError::Duplicate(id));
    }
    services.insert(id, service);
    Ok(())
}

fn descriptors<T: ?Sized + Described>(
    services: &BTreeMap<String, Arc<T>>,
) -> Vec<ServiceDescriptor> {
    services
        .values()
        .map(|service| service.descriptor().clone())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegistryManifest {
    pub producers: Vec<ServiceDescriptor>,
    pub filters: Vec<ServiceDescriptor>,
    pub transitions: Vec<ServiceDescriptor>,
    pub consumers: Vec<ServiceDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    InvalidId(String),
    Duplicate(String),
}

impl fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid media service registry: {self:?}")
    }
}

impl std::error::Error for RegistryError {}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestService(ServiceDescriptor);

    impl Producer for TestService {
        fn descriptor(&self) -> &ServiceDescriptor {
            &self.0
        }
    }

    #[test]
    fn registry_is_role_scoped_and_manifest_is_stable() {
        let service = Arc::new(TestService(
            ServiceDescriptor::new("file", "File", vec![MediaKind::Video]).unwrap(),
        ));
        let mut registry = PipelineRegistry::default();
        registry.register_producer(service.clone()).unwrap();

        assert_eq!(
            registry.producer("file").unwrap().descriptor().label,
            "File"
        );
        assert_eq!(registry.manifest().producers[0].id, "file");
        assert_eq!(
            registry.register_producer(service).unwrap_err(),
            RegistryError::Duplicate("file".to_owned())
        );
    }
}
