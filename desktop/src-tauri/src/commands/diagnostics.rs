//! Diagnostics preview/export edge of the boundary.
//!
//! Previews are redacted before display (`transaction::redacted_json`); the
//! export file is written only to an explicit user-selected destination.
//! Parent directories are never created implicitly (the transaction `export`
//! enforces an existing parent).

use std::{
    io,
    path::{Component, PathBuf},
};

use crate::{
    recovery::model::RecoveryEnvelopeV1,
    transaction::{export as write_export, preview, redacted_json, DiagnosticPreview},
};

use super::{CommandError, CODE_EXPORT_FAILED, CODE_EXPORT_PATH_REQUIRED};

/// Validate that the user explicitly selected an export file. An empty path
/// (no selection), a command-shaped string, a bare file name without a
/// folder, or a path ending in a folder separator all fail.
pub fn validate_export_path(value: &str) -> Result<(), CommandError> {
    if value.trim().is_empty() {
        return Err(CommandError::new(
            CODE_EXPORT_PATH_REQUIRED,
            "No export destination was chosen.",
            "Choose a file location in the save dialog first.",
        ));
    }
    super::reject_command_payload(value)?;
    let trimmed = value.trim();
    let has_folder = trimmed.contains('/') || trimmed.contains('\\');
    let last = trimmed.rsplit(['/', '\\']).next().unwrap_or_default();
    if !has_folder || last.is_empty() {
        return Err(CommandError::new(
            CODE_EXPORT_PATH_REQUIRED,
            "No export destination was chosen.",
            "Choose a file location in the save dialog first.",
        ));
    }
    Ok(())
}

pub fn map_export_error(error: io::Error) -> CommandError {
    use std::io::ErrorKind as K;
    match error.kind() {
        K::InvalidInput | K::NotFound => CommandError::new(
            CODE_EXPORT_PATH_REQUIRED,
            "That export destination is not usable.",
            "Choose an existing folder and a file name in the save dialog.",
        ),
        _ => CommandError::new(
            CODE_EXPORT_FAILED,
            "The diagnostic export could not be written.",
            "Pick a writable location and try again.",
        ),
    }
}

/// Harden a user-selected export destination: absolute paths only, no
/// parent-directory traversal, and an already-existing parent directory.
/// Shape checks run first; nothing is created or written here.
///
/// Handoff (Tasks 18/19): the webview has no file-dialog permission, so the
/// destination must arrive as an explicit absolute path from a UI affordance
/// the shell tasks provide (a future save-picker command or equivalent).
/// Until then, typed or relative paths fail closed here.
pub fn canonicalize_export_path(value: &str) -> Result<PathBuf, CommandError> {
    validate_export_path(value)?;
    let path = PathBuf::from(value.trim());
    if !path.is_absolute() {
        return Err(CommandError::new(
            CODE_EXPORT_PATH_REQUIRED,
            "The export destination must be an absolute file path.",
            "Choose a file location in the save dialog first.",
        ));
    }
    if path.components().any(|part| matches!(part, Component::ParentDir)) {
        return Err(CommandError::new(
            super::CODE_REJECTED_COMMAND_PAYLOAD,
            "That destination escapes its folder and was rejected.",
            "Choose a file location inside one folder.",
        ));
    }
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() && parent.is_dir() => Ok(path),
        _ => Err(CommandError::new(
            CODE_EXPORT_PATH_REQUIRED,
            "The export folder does not exist.",
            "Choose an existing folder and a file name in the save dialog.",
        )),
    }
}

/// The redacted diagnostic bundle served to preview/export handlers: the
/// structural preview (counts, op kinds, redacted fingerprint) plus the
/// canonical envelope with serials, fingerprints, and paths replaced. This
/// is the real redaction code path (`transaction::redacted_json`), not a
/// claim: nothing unredacted leaves through this helper.
pub struct PreviewBundle {
    pub preview: DiagnosticPreview,
    pub redacted_envelope: String,
}

pub fn preview_bundle(
    envelope: &RecoveryEnvelopeV1,
    model: &str,
    fingerprint: &str,
) -> PreviewBundle {
    PreviewBundle {
        preview: preview(envelope, model, fingerprint),
        redacted_envelope: redacted_json(envelope),
    }
}

/// Write already-redacted export contents to a validated destination. The
/// only filesystem write in the boundary; the transaction layer refuses
/// missing parents and never creates directories.
pub fn write_diagnostic_export(path: &PathBuf, redacted_contents: &str) -> Result<(), CommandError> {
    write_export(std::path::Path::new(path), redacted_contents).map_err(map_export_error)
}
