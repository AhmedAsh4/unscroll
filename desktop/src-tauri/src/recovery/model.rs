//! Recovery V1's deliberately small, dependency-free hostile-input boundary.

use std::collections::{BTreeMap, BTreeSet};

const MAX_INPUT: usize = 65_536;
const MAX_ITEMS: usize = 512;
const MAX_TEXT: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    Syntax,
    Schema,
    Field,
    Operation,
    Checksum,
    History,
    Baseline,
    DeviceBinding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Json {
    Null,
    Bool(bool),
    Number(u64),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryEnvelopeV1 {
    value: Json,
}

impl RecoveryEnvelopeV1 {
    pub fn parse(input: &str) -> Result<Self, ValidationError> {
        if input.len() > MAX_INPUT {
            return Err(ValidationError::Field);
        }
        let value = Parser::new(input).parse()?;
        let result = Self { value };
        result.validate()?;
        Ok(result)
    }

    pub fn parse_unchecked_checksum(input: &str) -> Result<Self, ValidationError> {
        if input.len() > MAX_INPUT {
            return Err(ValidationError::Field);
        }
        Ok(Self {
            value: Parser::new(input).parse()?,
        })
    }

    pub fn parse_for_device(
        input: &str,
        serial: &str,
        fingerprint: &str,
        user_id: u64,
    ) -> Result<Self, ValidationError> {
        text(serial)?;
        text(fingerprint)?;
        if user_id > 999_999 {
            return Err(ValidationError::DeviceBinding);
        }
        let envelope = Self::parse(input)?;
        let binding = object(
            envelope
                .part("device_binding")
                .ok_or(ValidationError::Schema)?,
            ValidationError::Schema,
        )?;
        if binding.get("serial") != Some(&Json::String(serial.into()))
            || binding.get("fingerprint") != Some(&Json::String(fingerprint.into()))
            || binding.get("user_id") != Some(&Json::Number(user_id))
        {
            return Err(ValidationError::DeviceBinding);
        }
        Ok(envelope)
    }

    pub fn canonical_json(&self) -> String {
        canonical(&self.value)
    }

    pub fn checksum(&self) -> String {
        self.string_at("checksum").unwrap_or_default().to_owned()
    }

    pub fn computed_checksum(&self) -> String {
        sha256_hex(canonical_without(&self.value, "checksum").as_bytes())
    }

    pub fn is_strict_prefix_of(&self, newer: &Self) -> Result<(), ValidationError> {
        self.validate()?;
        newer.validate()?;
        if self.part("baseline") != newer.part("baseline")
            || self.string_at("baseline_id") != newer.string_at("baseline_id")
        {
            return Err(ValidationError::Baseline);
        }
        if self.part("device_binding") != newer.part("device_binding") {
            return Err(ValidationError::Field);
        }
        let old = self.array_at("journal").ok_or(ValidationError::Schema)?;
        let new = newer.array_at("journal").ok_or(ValidationError::Schema)?;
        let old_revision = self.number_at("revision").ok_or(ValidationError::Schema)?;
        let new_revision = newer.number_at("revision").ok_or(ValidationError::Schema)?;
        if new_revision <= old_revision || new.len() < old.len() {
            return Err(ValidationError::History);
        }
        let mut changed = new.len() > old.len();
        for (before, after) in old.iter().zip(new.iter()) {
            if !same_entry_except_state(before, after)? {
                return Err(ValidationError::History);
            }
            let before_state = entry_state(before)?;
            let after_state = entry_state(after)?;
            if before_state != after_state {
                if before_state != "pending" || after_state != "applied" {
                    return Err(ValidationError::History);
                }
                changed = true;
            }
        }
        if !changed {
            return Err(ValidationError::History);
        }
        if newer.string_at("previous_revision_hash") != Some(&self.computed_checksum()) {
            return Err(ValidationError::History);
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), ValidationError> {
        let root = object(&self.value, ValidationError::Schema)?;
        exact_keys(
            root,
            &[
                "active_policy",
                "baseline",
                "baseline_id",
                "checksum",
                "device_binding",
                "journal",
                "maintenance",
                "previous_revision_hash",
                "revision",
                "schema_version",
            ],
            ValidationError::Schema,
        )?;
        if self.string_at("schema_version") != Some("recovery-v1") {
            return Err(ValidationError::Schema);
        }
        uuid(
            self.string_at("baseline_id")
                .ok_or(ValidationError::Field)?,
        )?;
        hash(
            self.string_at("checksum")
                .ok_or(ValidationError::Checksum)?,
        )?;
        validate_device(self.part("device_binding").ok_or(ValidationError::Schema)?)?;
        validate_baseline(
            self.part("baseline").ok_or(ValidationError::Schema)?,
            self.string_at("baseline_id").unwrap(),
        )?;
        validate_policy(
            self.part("active_policy").ok_or(ValidationError::Schema)?,
            self.part("baseline").unwrap(),
        )?;
        validate_maintenance(self.part("maintenance").ok_or(ValidationError::Schema)?)?;
        let revision = self.number_at("revision").ok_or(ValidationError::Field)?;
        let previous = self
            .part("previous_revision_hash")
            .ok_or(ValidationError::Schema)?;
        if revision == 0 {
            if previous != &Json::Null {
                return Err(ValidationError::History);
            }
        } else {
            hash(string(previous, ValidationError::History)?)?;
        }
        let journal = self.array_at("journal").ok_or(ValidationError::Schema)?;
        if journal.len() > MAX_ITEMS {
            return Err(ValidationError::History);
        }
        let mut ids = BTreeSet::new();
        let mut applied = 0u64;
        for entry in journal {
            let item = object(entry, ValidationError::Operation)?;
            exact_keys(
                item,
                &["id", "inverse", "operation", "state"],
                ValidationError::Operation,
            )?;
            let id = string(
                item.get("id").ok_or(ValidationError::Operation)?,
                ValidationError::Operation,
            )?;
            uuid(id)?;
            if !ids.insert(id) {
                return Err(ValidationError::Operation);
            }
            let state = string(
                item.get("state").ok_or(ValidationError::Operation)?,
                ValidationError::Operation,
            )?;
            if state != "pending" && state != "applied" {
                return Err(ValidationError::Operation);
            }
            if state == "applied" {
                applied += 1;
            }
            let operation = item.get("operation").ok_or(ValidationError::Operation)?;
            let inverse = item.get("inverse").ok_or(ValidationError::Operation)?;
            let kind = validate_operation(operation)?;
            if validate_operation(inverse)? != kind || !is_inverse(operation, inverse, kind)? {
                return Err(ValidationError::Operation);
            }
        }
        if revision != journal.len() as u64 + applied {
            return Err(ValidationError::History);
        }
        if self.checksum() != self.computed_checksum() {
            return Err(ValidationError::Checksum);
        }
        Ok(())
    }

    fn part(&self, key: &str) -> Option<&Json> {
        match &self.value {
            Json::Object(o) => o.get(key),
            _ => None,
        }
    }
    fn string_at(&self, key: &str) -> Option<&str> {
        self.part(key).and_then(|v| match v {
            Json::String(s) => Some(s.as_str()),
            _ => None,
        })
    }
    fn number_at(&self, key: &str) -> Option<u64> {
        self.part(key).and_then(|v| match v {
            Json::Number(n) => Some(*n),
            _ => None,
        })
    }
    fn array_at(&self, key: &str) -> Option<&[Json]> {
        self.part(key).and_then(|v| match v {
            Json::Array(a) => Some(a.as_slice()),
            _ => None,
        })
    }
}

fn entry_state(entry: &Json) -> Result<&str, ValidationError> {
    string(
        object(entry, ValidationError::History)?
            .get("state")
            .ok_or(ValidationError::History)?,
        ValidationError::History,
    )
}

fn same_entry_except_state(left: &Json, right: &Json) -> Result<bool, ValidationError> {
    let mut left = object(left, ValidationError::History)?.clone();
    let mut right = object(right, ValidationError::History)?.clone();
    left.remove("state");
    right.remove("state");
    Ok(left == right)
}

fn is_inverse(operation: &Json, inverse: &Json, kind: &str) -> Result<bool, ValidationError> {
    let operation = object(operation, ValidationError::Operation)?;
    let inverse = object(inverse, ValidationError::Operation)?;
    let equal = |key: &str| operation.get(key) == inverse.get(key);
    let opposite = |key: &str| matches!((operation.get(key), inverse.get(key)), (Some(Json::Bool(a)), Some(Json::Bool(b))) if a != b);
    Ok(match kind {
        "launcher_policy" => !equal("allowed_packages"),
        "package_suspension" => equal("package_id") && equal("user_id") && opposite("suspended"),
        "app_op" => equal("package_id") && equal("user_id") && equal("op") && !equal("mode"),
        "home" => !equal("component"),
        "maintenance" => opposite("open"),
        "cleanup" => equal("target") && opposite("removed"),
        _ => return Err(ValidationError::Operation),
    })
}

fn validate_device(value: &Json) -> Result<(), ValidationError> {
    let o = object(value, ValidationError::Field)?;
    exact_keys(
        o,
        &["fingerprint", "serial", "user_id"],
        ValidationError::Field,
    )?;
    text(string(
        o.get("serial").ok_or(ValidationError::Field)?,
        ValidationError::Field,
    )?)?;
    text(string(
        o.get("fingerprint").ok_or(ValidationError::Field)?,
        ValidationError::Field,
    )?)?;
    if number(o.get("user_id").ok_or(ValidationError::Field)?)? > 999_999 {
        return Err(ValidationError::Field);
    }
    Ok(())
}

fn validate_baseline(value: &Json, baseline_id: &str) -> Result<(), ValidationError> {
    let o = object(value, ValidationError::Baseline)?;
    exact_keys(
        o,
        &[
            "baseline_id",
            "baseline_launcher_package",
            "initial_home_component",
            "initial_packages",
        ],
        ValidationError::Baseline,
    )?;
    if string(
        o.get("baseline_id").ok_or(ValidationError::Baseline)?,
        ValidationError::Baseline,
    )? != baseline_id
    {
        return Err(ValidationError::Baseline);
    }
    package(string(
        o.get("baseline_launcher_package")
            .ok_or(ValidationError::Baseline)?,
        ValidationError::Baseline,
    )?)?;
    component(string(
        o.get("initial_home_component")
            .ok_or(ValidationError::Baseline)?,
        ValidationError::Baseline,
    )?)?;
    let packages = array(
        o.get("initial_packages").ok_or(ValidationError::Baseline)?,
        ValidationError::Baseline,
    )?;
    sorted_unique_packages(packages, ValidationError::Baseline, true)
}

fn validate_policy(value: &Json, baseline: &Json) -> Result<(), ValidationError> {
    let o = object(value, ValidationError::Field)?;
    exact_keys(
        o,
        &["allowed_packages", "baseline_launcher_package"],
        ValidationError::Field,
    )?;
    let launcher = string(
        o.get("baseline_launcher_package")
            .ok_or(ValidationError::Field)?,
        ValidationError::Field,
    )?;
    let original = object(baseline, ValidationError::Baseline)?
        .get("baseline_launcher_package")
        .unwrap();
    if Json::String(launcher.into()) != *original {
        return Err(ValidationError::Baseline);
    }
    sorted_unique_packages(
        array(
            o.get("allowed_packages").ok_or(ValidationError::Field)?,
            ValidationError::Field,
        )?,
        ValidationError::Field,
        false,
    )
}

fn sorted_unique_packages(
    values: &[Json],
    error: ValidationError,
    records: bool,
) -> Result<(), ValidationError> {
    if values.len() > MAX_ITEMS {
        return Err(error);
    }
    let mut previous = "";
    for value in values {
        let package_name = if records {
            let o = object(value, error.clone())?;
            exact_keys(o, &["package_id", "suspended", "user_id"], error.clone())?;
            if !matches!(o.get("suspended"), Some(Json::Bool(_)))
                || number(o.get("user_id").ok_or(error.clone())?)? > 999_999
            {
                return Err(error);
            }
            string(o.get("package_id").ok_or(error.clone())?, error.clone())?
        } else {
            string(value, error.clone())?
        };
        package(package_name)?;
        if !previous.is_empty() && previous >= package_name {
            return Err(error);
        }
        previous = package_name;
    }
    Ok(())
}

fn validate_maintenance(value: &Json) -> Result<(), ValidationError> {
    let o = object(value, ValidationError::Field)?;
    exact_keys(o, &["state"], ValidationError::Field)?;
    match string(
        o.get("state").ok_or(ValidationError::Field)?,
        ValidationError::Field,
    )? {
        "closed" | "open" => Ok(()),
        _ => Err(ValidationError::Field),
    }
}

fn validate_operation(value: &Json) -> Result<&str, ValidationError> {
    let o = object(value, ValidationError::Operation)?;
    let kind = string(
        o.get("kind").ok_or(ValidationError::Operation)?,
        ValidationError::Operation,
    )?;
    match kind {
        "launcher_policy" => {
            exact_keys(o, &["allowed_packages", "kind"], ValidationError::Operation)?;
            sorted_unique_packages(
                array(
                    o.get("allowed_packages").unwrap(),
                    ValidationError::Operation,
                )?,
                ValidationError::Operation,
                false,
            )?;
        }
        "package_suspension" => {
            exact_keys(
                o,
                &["kind", "package_id", "suspended", "user_id"],
                ValidationError::Operation,
            )?;
            package(string(
                o.get("package_id").unwrap(),
                ValidationError::Operation,
            )?)?;
            if !matches!(o.get("suspended"), Some(Json::Bool(_)))
                || number(o.get("user_id").unwrap())? > 999_999
            {
                return Err(ValidationError::Operation);
            }
        }
        "app_op" => {
            exact_keys(
                o,
                &["kind", "mode", "op", "package_id", "user_id"],
                ValidationError::Operation,
            )?;
            package(string(
                o.get("package_id").unwrap(),
                ValidationError::Operation,
            )?)?;
            text(string(o.get("op").unwrap(), ValidationError::Operation)?)?;
            text(string(o.get("mode").unwrap(), ValidationError::Operation)?)?;
            if number(o.get("user_id").unwrap())? > 999_999 {
                return Err(ValidationError::Operation);
            }
        }
        "home" => {
            exact_keys(o, &["component", "kind"], ValidationError::Operation)?;
            component(string(
                o.get("component").unwrap(),
                ValidationError::Operation,
            )?)?;
        }
        "maintenance" => {
            exact_keys(o, &["kind", "open"], ValidationError::Operation)?;
            if !matches!(o.get("open"), Some(Json::Bool(_))) {
                return Err(ValidationError::Operation);
            }
        }
        "cleanup" => {
            exact_keys(
                o,
                &["kind", "removed", "target"],
                ValidationError::Operation,
            )?;
            if !matches!(o.get("removed"), Some(Json::Bool(_)))
                || !matches!(
                    string(o.get("target").unwrap(), ValidationError::Operation)?,
                    "private_envelope" | "shared_envelope"
                )
            {
                return Err(ValidationError::Operation);
            }
        }
        _ => return Err(ValidationError::Operation),
    }
    Ok(kind)
}

fn object(
    value: &Json,
    error: ValidationError,
) -> Result<&BTreeMap<String, Json>, ValidationError> {
    if let Json::Object(o) = value {
        Ok(o)
    } else {
        Err(error)
    }
}
fn array(value: &Json, error: ValidationError) -> Result<&[Json], ValidationError> {
    if let Json::Array(a) = value {
        Ok(a)
    } else {
        Err(error)
    }
}
fn string(value: &Json, error: ValidationError) -> Result<&str, ValidationError> {
    if let Json::String(s) = value {
        Ok(s)
    } else {
        Err(error)
    }
}
fn number(value: &Json) -> Result<u64, ValidationError> {
    if let Json::Number(n) = value {
        Ok(*n)
    } else {
        Err(ValidationError::Field)
    }
}
fn exact_keys(
    o: &BTreeMap<String, Json>,
    keys: &[&str],
    error: ValidationError,
) -> Result<(), ValidationError> {
    if o.len() == keys.len() && keys.iter().all(|key| o.contains_key(*key)) {
        Ok(())
    } else {
        Err(error)
    }
}
fn text(value: &str) -> Result<(), ValidationError> {
    if !value.is_empty()
        && value.len() <= MAX_TEXT
        && value.bytes().all(|b| (0x20..=0x7e).contains(&b))
    {
        Ok(())
    } else {
        Err(ValidationError::Field)
    }
}
fn package(value: &str) -> Result<(), ValidationError> {
    text(value)?;
    let parts: Vec<_> = value.split('.').collect();
    if value.len() > 255
        || parts.len() < 2
        || parts.iter().any(|part| {
            part.is_empty()
                || !part.bytes().enumerate().all(|(index, byte)| {
                    byte == b'_'
                        || byte == b'$'
                        || (byte.is_ascii_alphanumeric()
                            && (index > 0 || byte.is_ascii_alphabetic() || byte == b'_'))
                })
        })
    {
        Err(ValidationError::Field)
    } else {
        Ok(())
    }
}
fn component(value: &str) -> Result<(), ValidationError> {
    text(value)?;
    let Some((package_name, class)) = value.split_once('/') else {
        return Err(ValidationError::Field);
    };
    package(package_name)?;
    if class.is_empty()
        || class.len() > MAX_TEXT
        || !class
            .bytes()
            .all(|b| b == b'.' || b == b'$' || b == b'_' || b.is_ascii_alphanumeric())
    {
        Err(ValidationError::Field)
    } else {
        Ok(())
    }
}
fn uuid(value: &str) -> Result<(), ValidationError> {
    if value.len() == 36
        && [8, 13, 18, 23].iter().all(|&i| value.as_bytes()[i] == b'-')
        && value.bytes().enumerate().all(|(i, b)| {
            [8, 13, 18, 23].contains(&i) || b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
        })
    {
        Ok(())
    } else {
        Err(ValidationError::Field)
    }
}
fn hash(value: &str) -> Result<(), ValidationError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Ok(())
    } else {
        Err(ValidationError::Checksum)
    }
}

fn canonical_without(value: &Json, removed: &str) -> String {
    match value {
        Json::Object(o) => canonical(&Json::Object(
            o.iter()
                .filter(|(k, _)| k.as_str() != removed)
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        )),
        _ => canonical(value),
    }
}
fn canonical(value: &Json) -> String {
    match value {
        Json::Null => "null".into(),
        Json::Bool(v) => v.to_string(),
        Json::Number(v) => v.to_string(),
        Json::String(v) => format!(
            "\"{}\"",
            v.replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\r', "\\r")
                .replace('\t', "\\t")
        ),
        Json::Array(a) => format!(
            "[{}]",
            a.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        Json::Object(o) => format!(
            "{{{}}}",
            o.iter()
                .map(|(k, v)| format!("\"{}\":{}", k, canonical(v)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            bytes: input.as_bytes(),
            at: 0,
        }
    }
    fn parse(mut self) -> Result<Json, ValidationError> {
        let value = self.value(0)?;
        self.ws();
        if self.at == self.bytes.len() {
            Ok(value)
        } else {
            Err(ValidationError::Syntax)
        }
    }
    fn value(&mut self, depth: u8) -> Result<Json, ValidationError> {
        if depth > 32 {
            return Err(ValidationError::Syntax);
        };
        self.ws();
        match self.peek() {
            Some(b'{') => self.object(depth + 1),
            Some(b'[') => self.array(depth + 1),
            Some(b'\"') => Ok(Json::String(self.string()?)),
            Some(b't') => {
                self.word(b"true")?;
                Ok(Json::Bool(true))
            }
            Some(b'f') => {
                self.word(b"false")?;
                Ok(Json::Bool(false))
            }
            Some(b'n') => {
                self.word(b"null")?;
                Ok(Json::Null)
            }
            Some(b'0'..=b'9') => Ok(Json::Number(self.number()?)),
            _ => Err(ValidationError::Syntax),
        }
    }
    fn object(&mut self, depth: u8) -> Result<Json, ValidationError> {
        self.take(b'{')?;
        self.ws();
        let mut out = BTreeMap::new();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Json::Object(out));
        }
        loop {
            self.ws();
            let key = self.string()?;
            self.ws();
            self.take(b':')?;
            let value = self.value(depth)?;
            if out.insert(key, value).is_some() {
                return Err(ValidationError::Syntax);
            };
            self.ws();
            match self.next() {
                Some(b',') => (),
                Some(b'}') => break,
                _ => return Err(ValidationError::Syntax),
            }
        }
        Ok(Json::Object(out))
    }
    fn array(&mut self, depth: u8) -> Result<Json, ValidationError> {
        self.take(b'[')?;
        self.ws();
        let mut out = Vec::new();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Json::Array(out));
        }
        loop {
            if out.len() >= MAX_ITEMS {
                return Err(ValidationError::Field);
            };
            out.push(self.value(depth)?);
            self.ws();
            match self.next() {
                Some(b',') => (),
                Some(b']') => break,
                _ => return Err(ValidationError::Syntax),
            }
        }
        Ok(Json::Array(out))
    }
    fn string(&mut self) -> Result<String, ValidationError> {
        self.take(b'\"')?;
        let mut out = String::new();
        loop {
            let byte = self.next().ok_or(ValidationError::Syntax)?;
            match byte {
                b'\"' => break,
                b'\\' => match self.next().ok_or(ValidationError::Syntax)? {
                    b'\"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\u{8}'),
                    b'f' => out.push('\u{c}'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        let mut n = 0u32;
                        for _ in 0..4 {
                            n = n * 16 + hex(self.next().ok_or(ValidationError::Syntax)?)? as u32;
                        }
                        out.push(char::from_u32(n).ok_or(ValidationError::Syntax)?);
                    }
                    _ => return Err(ValidationError::Syntax),
                },
                0..=0x1f => return Err(ValidationError::Syntax),
                b => out.push(b as char),
            }
            if out.len() > MAX_TEXT {
                return Err(ValidationError::Field);
            }
        }
        Ok(out)
    }
    fn number(&mut self) -> Result<u64, ValidationError> {
        let start = self.at;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.at += 1
        }
        if self.at - start > 1 && self.bytes[start] == b'0' {
            return Err(ValidationError::Syntax);
        };
        std::str::from_utf8(&self.bytes[start..self.at])
            .ok()
            .and_then(|s| s.parse().ok())
            .ok_or(ValidationError::Syntax)
    }
    fn word(&mut self, word: &[u8]) -> Result<(), ValidationError> {
        if self.bytes.get(self.at..self.at + word.len()) == Some(word) {
            self.at += word.len();
            Ok(())
        } else {
            Err(ValidationError::Syntax)
        }
    }
    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.at += 1
        }
    }
    fn take(&mut self, wanted: u8) -> Result<(), ValidationError> {
        if self.next() == Some(wanted) {
            Ok(())
        } else {
            Err(ValidationError::Syntax)
        }
    }
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }
    fn next(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.at += 1;
        Some(b)
    }
}
fn hex(value: u8) -> Result<u8, ValidationError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(ValidationError::Syntax),
    }
}

pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut bytes = data.to_vec();
    let bits = (bytes.len() as u64) * 8;
    bytes.push(0x80);
    while bytes.len() % 64 != 56 {
        bytes.push(0)
    }
    bytes.extend_from_slice(&bits.to_be_bytes());
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    for block in bytes.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[i * 4..i * 4 + 4].try_into().unwrap())
        }
        for i in 16..64 {
            w[i] = w[i - 16]
                .wrapping_add(
                    w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3),
                )
                .wrapping_add(w[i - 7])
                .wrapping_add(
                    w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10),
                );
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut q) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let t1 = q
                .wrapping_add(e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25))
                .wrapping_add((e & f) ^ (!e & g))
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let t2 = (a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22))
                .wrapping_add((a & b) ^ (a & c) ^ (b & c));
            q = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(q);
    }
    h.iter().map(|v| format!("{v:08x}")).collect()
}
