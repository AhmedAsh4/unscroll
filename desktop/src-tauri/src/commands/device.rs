//! Device discovery/inspection edge of the boundary.
//!
//! Thin only: validate UI strings, gate on the inspected session binding,
//! map service errors to [`CommandError`] DTOs. No ADB is constructed here.

use crate::{
    adb::{Fingerprint, Serial},
    app_state::UnscrollState,
    device::{DiscoveryError, InspectionError},
};

use super::{validate_fingerprint, validate_serial, CommandError, CODE_DEVICE_UNAVAILABLE};

/// Validate an inspection request without touching the device.
pub fn validate_inspect_request(
    serial: &str,
    fingerprint: &str,
) -> Result<(Serial, Fingerprint), CommandError> {
    Ok((validate_serial(serial)?, validate_fingerprint(fingerprint)?))
}

/// Bind the session after a successful inspection. Refused while a
/// mutation holds the session; inspection handlers win the mutation guard
/// first, so this atomic check closes the rebind race.
pub fn bind_session(
    state: &UnscrollState,
    serial: &str,
    fingerprint: &str,
) -> Result<(), CommandError> {
    let (serial, fingerprint) = validate_inspect_request(serial, fingerprint)?;
    state.bind_device(serial.as_str(), fingerprint.as_str())
}

/// Reject operations for a replaced device (stale or mismatched ids).
pub fn check_session(
    state: &UnscrollState,
    serial: &str,
    fingerprint: &str,
) -> Result<(), CommandError> {
    let (serial, fingerprint) = validate_inspect_request(serial, fingerprint)?;
    state.check_binding(serial.as_str(), fingerprint.as_str())
}

/// Record a fresh inspection: rebind the session and release the previous
/// catalog's icon memory, which may all have changed.
pub fn note_inspection(
    state: &UnscrollState,
    serial: &str,
    fingerprint: &str,
) -> Result<(), CommandError> {
    bind_session(state, serial, fingerprint)?;
    state.clear_icons();
    Ok(())
}

pub fn map_discovery_error(error: &DiscoveryError) -> CommandError {
    use super::{
        CODE_MISSING_DRIVER, CODE_MULTIPLE_DEVICES, CODE_NO_DEVICE,
        CODE_UNAUTHORIZED_DEVICE, CODE_UNSUPPORTED_DEVICE,
    };
    match error {
        DiscoveryError::NoDevice => CommandError::new(
            CODE_NO_DEVICE,
            "No phone was found over USB.",
            "Connect one phone and enable USB debugging.",
        ),
        DiscoveryError::MultipleDevices => CommandError::new(
            CODE_MULTIPLE_DEVICES,
            "More than one phone is connected.",
            "Leave exactly one phone connected and try again.",
        ),
        DiscoveryError::Unauthorized => CommandError::new(
            CODE_UNAUTHORIZED_DEVICE,
            "The phone is waiting for USB-debugging authorization.",
            "Accept the authorization prompt on the phone screen.",
        ),
        DiscoveryError::MissingDriver => CommandError::new(
            CODE_MISSING_DRIVER,
            "Windows cannot talk to the phone (missing driver).",
            "Follow the manufacturer driver guidance, then reconnect.",
        ),
        DiscoveryError::UnsupportedApi => CommandError::new(
            CODE_UNSUPPORTED_DEVICE,
            "This Android version is outside capability-tested Android 7-16.",
            "Use a capability-tested phone, or wait for a newer Unscroll release.",
        ),
        _ => CommandError::new(
            CODE_DEVICE_UNAVAILABLE,
            "The phone stopped responding during discovery.",
            "Check the USB cable, then try again.",
        ),
    }
}

pub fn map_inspection_error(error: &InspectionError) -> CommandError {
    use super::{
        CODE_INCOMPATIBLE_LAUNCHER, CODE_PREFLIGHT_FAILED, CODE_RECOVERY_REQUIRED,
        CODE_UNSUPPORTED_DEVICE,
    };
    match error {
        InspectionError::Discovery => CommandError::new(
            CODE_DEVICE_UNAVAILABLE,
            "The phone stopped responding during inspection.",
            "Check the USB cable, then try again.",
        ),
        InspectionError::UnsupportedApi => CommandError::new(
            CODE_UNSUPPORTED_DEVICE,
            "This Android version is outside capability-tested Android 7-16.",
            "Use a capability-tested phone, or wait for a newer Unscroll release.",
        ),
        InspectionError::RecoveryRequired => CommandError::new(
            CODE_RECOVERY_REQUIRED,
            "This phone already holds Unscroll recovery data.",
            "Reconnect and choose edit, maintenance, or restore instead of a new setup.",
        ),
        InspectionError::SignatureMismatch | InspectionError::BridgeMismatch => {
            CommandError::new(
                CODE_INCOMPATIBLE_LAUNCHER,
                "The launcher on the phone is not compatible with this Unscroll release.",
                "Install the matching Unscroll release, then try again.",
            )
        }
        _ => CommandError::new(
            CODE_PREFLIGHT_FAILED,
            "The phone failed a preflight check; nothing was changed.",
            "Follow the on-screen guidance for the failed check and retry.",
        ),
    }
}
