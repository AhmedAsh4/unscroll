//! Local-only diagnostic preview, redaction, and explicit export.
//!
//! Diagnostics are never uploaded. Callers preview redacted metadata first and
//! only write the export when the user chooses an explicit destination file.
//!
//! This module also hosts the shared read-only envelope view (`parse_envelope`)
//! used by edit and restore to interpret journal entries without depending on
//! device access.
//!
//! Envelope validity always comes from `RecoveryEnvelopeV1::parse` (checksum,
//! schema, and history). The structural walk below is a deliberately minimal,
//! documented ASCII-only reader over the canonical JSON the model guarantees
//! (`text`/`package`/`component` reject anything outside `0x20..=0x7E`, so all
//! structural characters are single-byte). It replaces a duplicated full JSON
//! parser/serializer; strings are decoded via `chars` (not `byte as char`), so
//! non-ASCII — should it ever appear — is preserved rather than mangled. Only
//! `u64` numbers occur in the model, so only those are accepted.

use std::io;
use std::path::Path;

use crate::recovery::model::RecoveryEnvelopeV1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticPreview {
    pub device_model: String,
    pub fingerprint_redacted: String,
    /// Length of the active allowlist (not a device-wide package total).
    pub allowlist_count: usize,
    /// Number of baseline packages recorded at setup, when readable.
    pub initial_package_count: usize,
    pub operations: Vec<String>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn preview(envelope: &RecoveryEnvelopeV1, model: &str, fingerprint: &str) -> DiagnosticPreview {
    // Typed accessor for the allowlist count; journal details come from the
    // validated structural walk (no per-entry accessor exists on the model).
    let allowlist_count = envelope.active_allowed_packages().len();
    let parsed = parse_envelope(&envelope.canonical_json());
    let (operations, errors, warnings, initial_package_count) = match &parsed {
        Some(view) => (
            view.entries.iter().map(|entry| format!("{} {}", entry.op_kind, entry.state)).collect(),
            view.entries
                .iter()
                .filter(|entry| entry.state == "failed")
                .map(|entry| format!("{} {} failed", entry.id, entry.op_kind))
                .collect(),
            view.entries
                .iter()
                .filter(|entry| entry.state == "pending")
                .map(|entry| format!("{} {} pending", entry.id, entry.op_kind))
                .collect(),
            view.initial_packages.len(),
        ),
        None => (Vec::new(), Vec::new(), Vec::new(), 0),
    };
    DiagnosticPreview {
        device_model: model.into(),
        fingerprint_redacted: redact_fingerprint(fingerprint),
        allowlist_count,
        initial_package_count,
        operations,
        errors,
        warnings,
    }
}

fn redact_fingerprint(fingerprint: &str) -> String {
    if fingerprint.len() > 12 {
        let mut end = 8;
        while end < fingerprint.len() && !fingerprint.is_char_boundary(end) {
            end += 1;
        }
        format!("{}…", &fingerprint[..end])
    } else {
        "REDACTED".into()
    }
}

/// Canonical envelope JSON with sensitive values redacted.
///
/// Validated with `RecoveryEnvelopeV1::parse` first; redaction itself is
/// key-name string surgery on the canonical string (no re-serialization, so no
/// serializer bugs). Serials, fingerprints, and path-like values are replaced.
/// Package IDs, operation kinds, journal states, components, and the revision
/// chain are kept so the redacted copy remains useful for manual review.
/// The redacted output is for human review only and is not expected to
/// re-validate (checksums cover the original values).
pub fn redacted_json(envelope: &RecoveryEnvelopeV1) -> String {
    let canonical = envelope.canonical_json();
    if RecoveryEnvelopeV1::parse(&canonical).is_err() {
        return "{}".into();
    }
    redact_canonical(&canonical)
}

fn redact_canonical(canonical: &str) -> String {
    let bytes = canonical.as_bytes();
    let mut out = String::with_capacity(canonical.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let Some(end) = raw_string_end(canonical, i) else {
                let ch = canonical[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
                continue;
            };
            let raw = &canonical[i..end];
            let Some(key) = unescape_string(raw) else {
                out.push_str(raw);
                i = end;
                continue;
            };
            let after = skip_ws(canonical, end);
            if after < bytes.len() && bytes[after] == b':' {
                out.push_str(raw);
                out.push(':');
                i = skip_ws(canonical, after + 1);
                if i < bytes.len() && bytes[i] == b'"' {
                    let Some(vend) = raw_string_end(canonical, i) else {
                        continue;
                    };
                    let vraw = &canonical[i..vend];
                    let redact = match unescape_string(vraw) {
                        Some(value) => {
                            key == "serial"
                                || key == "fingerprint"
                                || (value.contains('/')
                                    && key != "component"
                                    && key != "initial_home_component")
                        }
                        None => false,
                    };
                    if redact {
                        out.push_str("\"REDACTED\"");
                    } else {
                        out.push_str(vraw);
                    }
                    i = vend;
                }
            } else {
                out.push_str(raw);
                i = end;
            }
        } else {
            let ch = canonical[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// Write diagnostic contents only to an explicit, already-existing location.
///
/// Refuses empty paths, paths without a file name, and paths whose parent is
/// empty or missing. Parent directories are never created implicitly.
pub fn export(path: &Path, contents: &str) -> io::Result<()> {
    if path.as_os_str().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "diagnostic export needs an explicit file path",
        ));
    }
    if path.file_name().is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "diagnostic export needs a file name",
        ));
    }
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() && parent.is_dir() => (),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "diagnostic export parent directory must already exist",
            ));
        }
    }
    std::fs::write(path, contents)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ParsedOp {
    Suspend { package: String, user: u64, suspended: bool },
    AppOp { package: String, user: u64, op: String, mode: String },
    Home { component: String },
    LauncherPolicy { allowed: Vec<String> },
    Maintenance { open: bool },
    Cleanup { target: String, removed: bool },
    Other { kind: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedEntry {
    pub id: String,
    pub state: String,
    pub op_kind: String,
    pub op: ParsedOp,
    pub inverse: ParsedOp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedEnvelope {
    pub entries: Vec<ParsedEntry>,
    pub initial_home: String,
    pub initial_packages: Vec<String>,
    pub initial_suspended: Vec<String>,
    pub active_allowed: Vec<String>,
}

pub(crate) fn parse_envelope(canonical: &str) -> Option<ParsedEnvelope> {
    // Single source of truth for validity; structural walk only reads.
    let envelope = RecoveryEnvelopeV1::parse(canonical).ok()?;
    let active_allowed = envelope.active_allowed_packages();
    let journal_raw = extract_raw_value(canonical, "journal")?;
    let mut entries = Vec::new();
    for item in extract_array_items(journal_raw)? {
        let op_raw = extract_raw_value(item, "operation")?;
        entries.push(ParsedEntry {
            id: extract_string_field(item, "id")?,
            state: extract_string_field(item, "state")?,
            op_kind: extract_string_field(op_raw, "kind").unwrap_or("unknown".into()),
            op: parse_op(op_raw)?,
            inverse: parse_op(extract_raw_value(item, "inverse")?)?,
        });
    }
    let baseline_raw = extract_raw_value(canonical, "baseline")?;
    let initial_home = extract_string_field(baseline_raw, "initial_home_component")?;
    let mut initial_packages = Vec::new();
    let mut initial_suspended = Vec::new();
    for item in extract_array_items(extract_raw_value(baseline_raw, "initial_packages")?)? {
        let package = extract_string_field(item, "package_id")?;
        if extract_bool_field(item, "suspended")? {
            initial_suspended.push(package.clone());
        }
        initial_packages.push(package);
    }
    Some(ParsedEnvelope { entries, initial_home, initial_packages, initial_suspended, active_allowed })
}

fn parse_op(raw: &str) -> Option<ParsedOp> {
    let kind = extract_string_field(raw, "kind")?;
    Some(match kind.as_str() {
        "package_suspension" => ParsedOp::Suspend {
            package: extract_string_field(raw, "package_id")?,
            user: extract_u64_field(raw, "user_id")?,
            suspended: extract_bool_field(raw, "suspended")?,
        },
        "app_op" => ParsedOp::AppOp {
            package: extract_string_field(raw, "package_id")?,
            user: extract_u64_field(raw, "user_id")?,
            op: extract_string_field(raw, "op")?,
            mode: extract_string_field(raw, "mode")?,
        },
        "home" => ParsedOp::Home { component: extract_string_field(raw, "component")? },
        "launcher_policy" => {
            let mut allowed = Vec::new();
            for item in extract_array_items(extract_raw_value(raw, "allowed_packages")?)? {
                allowed.push(unescape_string(item.trim())?);
            }
            ParsedOp::LauncherPolicy { allowed }
        }
        "maintenance" => ParsedOp::Maintenance { open: extract_bool_field(raw, "open")? },
        "cleanup" => ParsedOp::Cleanup {
            target: extract_string_field(raw, "target")?,
            removed: extract_bool_field(raw, "removed")?,
        },
        _ => ParsedOp::Other { kind },
    })
}

// --- Minimal ASCII-only structural walk ------------------------------------
// Canonical JSON is compact (`{"k":v,...}`, no pretty whitespace) with
// `BTreeMap`-sorted keys, but the walk tolerates optional whitespace so it
// does not depend on exact formatting. All structural bytes (`{}[]":,`) are
// ASCII; model validation guarantees string contents are `0x20..=0x7E`.

fn skip_ws(s: &str, mut i: usize) -> usize {
    let bytes = s.as_bytes();
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\n' | b'\r' | b'\t') {
        i += 1;
    }
    i
}

fn raw_string_end(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    if bytes.get(start) != Some(&b'"') {
        return None;
    }
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => return Some(i + 1),
            b'\\' => {
                i += 2;
                if i > bytes.len() {
                    return None;
                }
            }
            0x00..=0x1f => return None,
            _ => i += 1,
        }
    }
    None
}

fn unescape_string(raw: &str) -> Option<String> {
    if raw.len() < 2 || !raw.starts_with('"') || !raw.ends_with('"') {
        return None;
    }
    let inner = &raw[1..raw.len() - 1];
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next()? {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '/' => out.push('/'),
                'b' => out.push('\u{8}'),
                'f' => out.push('\u{c}'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if hex.len() != 4 {
                        return None;
                    }
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                _ => return None,
            }
        } else {
            if (c as u32) < 0x20 {
                return None;
            }
            out.push(c);
        }
    }
    Some(out)
}

fn raw_value_end(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let i = skip_ws(s, start);
    match bytes.get(i)? {
        b'"' => raw_string_end(s, i),
        b'{' | b'[' => {
            let mut depth = 0usize;
            let mut j = i;
            while j < bytes.len() {
                match bytes[j] {
                    b'"' => j = raw_string_end(s, j)?,
                    b'{' | b'[' => {
                        depth += 1;
                        j += 1;
                    }
                    b'}' | b']' => {
                        if depth == 0 {
                            return None;
                        }
                        depth -= 1;
                        j += 1;
                        if depth == 0 {
                            return Some(j);
                        }
                    }
                    _ => j += 1,
                }
            }
            None
        }
        _ => {
            let mut j = i;
            while j < bytes.len() && !matches!(bytes[j], b',' | b'}' | b']') {
                j += 1;
            }
            if j == i {
                None
            } else {
                Some(j)
            }
        }
    }
}

/// Raw JSON slice of a top-level `key` inside an object slice.
fn extract_raw_value<'a>(obj: &'a str, key: &str) -> Option<&'a str> {
    let bytes = obj.as_bytes();
    let mut i = skip_ws(obj, 0);
    if bytes.get(i) != Some(&b'{') {
        return None;
    }
    i += 1;
    loop {
        i = skip_ws(obj, i);
        if i < bytes.len() && bytes[i] == b'}' {
            return None;
        }
        let kend = raw_string_end(obj, i)?;
        let name = unescape_string(&obj[i..kend])?;
        i = skip_ws(obj, kend);
        if bytes.get(i) != Some(&b':') {
            return None;
        }
        i = skip_ws(obj, i + 1);
        let vend = raw_value_end(obj, i)?;
        if name == key {
            return Some(obj[i..vend].trim());
        }
        i = skip_ws(obj, vend);
        match bytes.get(i)? {
            b',' => i += 1,
            b'}' => return None,
            _ => return None,
        }
    }
}

fn extract_array_items(arr: &str) -> Option<Vec<&str>> {
    let trimmed = arr.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return None;
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    if inner.trim().is_empty() {
        return Some(Vec::new());
    }
    let mut items = Vec::new();
    let mut i = 0;
    loop {
        i = skip_ws(inner, i);
        if i >= inner.len() {
            break;
        }
        let end = raw_value_end(inner, i)?;
        items.push(inner[i..end].trim());
        i = skip_ws(inner, end);
        if i >= inner.len() {
            break;
        }
        match inner.as_bytes().get(i)? {
            b',' => i += 1,
            _ => return None,
        }
    }
    Some(items)
}

fn extract_string_field(obj: &str, key: &str) -> Option<String> {
    unescape_string(extract_raw_value(obj, key)?.trim())
}

fn extract_bool_field(obj: &str, key: &str) -> Option<bool> {
    match extract_raw_value(obj, key)?.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn extract_u64_field(obj: &str, key: &str) -> Option<u64> {
    extract_raw_value(obj, key)?.trim().parse().ok()
}
