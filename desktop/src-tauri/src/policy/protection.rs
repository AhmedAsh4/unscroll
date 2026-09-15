use std::collections::BTreeSet;
use crate::{adb::PackageId, device::DeviceSnapshot};

pub(crate) fn protected(snapshot: &DeviceSnapshot) -> BTreeSet<String> {
    let mut packages = snapshot.protected.iter().map(|fact| fact.package.as_str().to_owned()).collect::<BTreeSet<_>>();
    packages.insert("org.unscroll.launcher".into());
    packages.insert(snapshot.home.as_str().split('/').next().unwrap_or_default().into());
    packages
}

pub(crate) fn may_suspend(snapshot: &DeviceSnapshot, package: &PackageId) -> bool {
    !protected(snapshot).contains(package.as_str())
}
