mod bootstrap;
mod capabilities;
mod catalog;
mod discovery;
mod errors;
mod identity;
mod inspect;

pub use bootstrap::LauncherArtifact;
pub use capabilities::CapabilityReport;
pub use catalog::{AppCatalogEntry, InstallSourceFact, ProtectedPackageFact, StoreFact};
pub use discovery::{discover, Discovery};
pub use errors::{classify_bootstrap, classify_transport, BootstrapError, DiscoveryError};
pub use identity::{classify_api, ApiSupport, DeviceIdentity};
pub use inspect::{
    inspect, DeviceSnapshot, InspectionError, ProfileFact, RecoveryObservation, XiaomiGuidance,
};
