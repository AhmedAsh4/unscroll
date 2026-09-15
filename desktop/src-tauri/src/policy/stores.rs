use std::collections::BTreeSet;
use crate::device::DeviceSnapshot;

/// Stores come from inspected intent/role facts; names are deliberately not guessed here.
pub(crate) fn packages(snapshot: &DeviceSnapshot) -> BTreeSet<String> {
    snapshot.stores.iter().map(|fact| fact.package.as_str().to_owned()).collect()
}
