use crate::adb::{Component, Cursor, Fingerprint, PackageId, Serial, StreamId};

const MAX_ICON_BYTES: usize = 1_048_576;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppCatalogEntry {
    pub package: PackageId,
    pub component: Component,
    pub label: String,
    pub icon: Vec<u8>,
    pub suspended: bool,
    pub enabled: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectedPackageFact {
    pub package: PackageId,
    pub reason: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreFact {
    pub package: PackageId,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallSourceFact {
    pub package: PackageId,
    pub allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IconRequest {
    pub package: PackageId,
    pub component: Component,
    pub stream: StreamId,
    pub length: usize,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BridgeFacts {
    pub package_suspension: bool,
    pub home_selection: bool,
    pub app_ops: bool,
    pub recovery_storage: bool,
}
type ParsedCatalogEntry = (AppCatalogEntry, Option<ProtectedPackageFact>, bool);
type CatalogPage = (Vec<ParsedCatalogEntry>, Option<Cursor>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BridgeReply {
    Missing,
    Value(Json),
}

pub(crate) fn health(text: &str) -> Result<String, ()> {
    let result = reply(text)?;
    let Json::Object(result) = result else {
        return Err(());
    };
    exact(
        &result,
        &[
            "launcher_package",
            "launcher_signing_sha256",
            "protocol_version",
            "recovery_schema",
        ],
    )?;
    let signer = string(&result, "launcher_signing_sha256")?;
    (string(&result, "launcher_package")? == "org.unscroll.launcher"
        && string(&result, "protocol_version")? == "bridge-v1"
        && string(&result, "recovery_schema")? == "recovery-v1"
        && signer.len() == 64
        && signer
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
    .then(|| signer.to_owned())
    .ok_or(())
}

pub(crate) fn facts(
    text: &str,
    serial: &Serial,
    fingerprint: &Fingerprint,
) -> Result<BridgeFacts, ()> {
    let Json::Object(result) = reply(text)? else {
        return Err(());
    };
    exact(&result, &["capabilities", "device_binding"])?;
    let Json::Object(binding) = result.get("device_binding").ok_or(())? else {
        return Err(());
    };
    exact(binding, &["fingerprint", "serial", "user_id"])?;
    if string(binding, "serial")? != serial.as_str()
        || string(binding, "fingerprint")? != fingerprint.as_str()
        || number(binding, "user_id")? != 0
    {
        return Err(());
    }
    let Json::Object(caps) = result.get("capabilities").ok_or(())? else {
        return Err(());
    };
    exact(
        caps,
        &[
            "app_ops",
            "home_selection",
            "package_suspension",
            "recovery_storage",
        ],
    )?;
    Ok(BridgeFacts {
        app_ops: boolean(caps, "app_ops")?,
        home_selection: boolean(caps, "home_selection")?,
        package_suspension: boolean(caps, "package_suspension")?,
        recovery_storage: boolean(caps, "recovery_storage")?,
    })
}

pub(crate) fn catalog(text: &str) -> Result<CatalogPage, ()> {
    let Json::Object(result) = reply(text)? else {
        return Err(());
    };
    exact(&result, &["entries", "next_cursor"])?;
    let Json::Array(entries) = result.get("entries").ok_or(())? else {
        return Err(());
    };
    if entries.len() > 100 {
        return Err(());
    }
    let next = match result.get("next_cursor").ok_or(())? {
        Json::Null => None,
        Json::String(value) => Some(Cursor::parse(value).map_err(|_| ())?),
        _ => return Err(()),
    };
    entries
        .iter()
        .map(entry)
        .collect::<Result<Vec<_>, _>>()
        .map(|entries| (entries, next))
}

fn entry(value: &Json) -> Result<ParsedCatalogEntry, ()> {
    let Json::Object(value) = value else {
        return Err(());
    };
    exact(
        value,
        &[
            "activity_icon_available",
            "activity_name",
            "application_icon_available",
            "enabled",
            "icon_available",
            "label",
            "launchable",
            "package_id",
            "protected_reason",
            "suspended",
            "user_id",
        ],
    )?;
    if !boolean(value, "launchable")? || number(value, "user_id")? != 0 {
        return Err(());
    }
    let package = PackageId::parse(string(value, "package_id")?).map_err(|_| ())?;
    let component = Component::parse(&format!(
        "{}/{}",
        package.as_str(),
        string(value, "activity_name")?
    ))
    .map_err(|_| ())?;
    let label = string(value, "label")?.to_owned();
    if label.len() > 512 {
        return Err(());
    }
    let protected = match value.get("protected_reason").ok_or(())? {
        Json::Null => None,
        Json::String(reason) if !reason.is_empty() && reason.len() <= 512 => {
            Some(ProtectedPackageFact {
                package: package.clone(),
                reason: reason.clone(),
            })
        }
        _ => return Err(()),
    };
    Ok((
        AppCatalogEntry {
            package,
            component,
            label,
            icon: Vec::new(),
            suspended: boolean(value, "suspended")?,
            enabled: boolean(value, "enabled")?,
        },
        protected,
        boolean(value, "icon_available")?,
    ))
}

pub(crate) fn icon(
    text: &str,
    package: PackageId,
    component: Component,
) -> Result<IconRequest, ()> {
    let Json::Object(result) = reply(text)? else {
        return Err(());
    };
    exact(
        &result,
        &["byte_length", "mime_type", "sha256", "stream_id"],
    )?;
    let length = number(&result, "byte_length")? as usize;
    if !(1..=MAX_ICON_BYTES).contains(&length) || string(&result, "mime_type")? != "image/png" {
        return Err(());
    }
    let sha256 = string(&result, "sha256")?.to_owned();
    if sha256.len() != 64
        || !sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(());
    }
    Ok(IconRequest {
        package,
        component,
        stream: StreamId::parse(string(&result, "stream_id")?).map_err(|_| ())?,
        length,
        sha256,
    })
}

pub(crate) fn envelope(text: &str) -> Result<BridgeReply, ()> {
    match frame(text)? {
        Json::Object(root) if root.get("ok") == Some(&Json::Bool(false)) => {
            let Json::Object(error) = root.get("error").ok_or(())? else {
                return Err(());
            };
            (string(error, "code")? == "not_found")
                .then_some(BridgeReply::Missing)
                .ok_or(())
        }
        Json::Object(root) if root.get("ok") == Some(&Json::Bool(true)) => {
            Ok(BridgeReply::Value(root.get("result").cloned().ok_or(())?))
        }
        _ => Err(()),
    }
}

pub(crate) fn reply(text: &str) -> Result<Json, ()> {
    let Json::Object(root) = frame(text)? else {
        return Err(());
    };
    exact(&root, &["ok", "protocol_version", "result"])?;
    if root.get("protocol_version") != Some(&Json::String("bridge-v1".into()))
        || root.get("ok") != Some(&Json::Bool(true))
    {
        return Err(());
    }
    root.get("result").cloned().ok_or(())
}

fn frame(text: &str) -> Result<Json, ()> {
    let start = text.find("response=").map(|at| at + 9).unwrap_or(0);
    let text = &text[start..];
    let start = text.find('{').ok_or(())?;
    let bytes = &text.as_bytes()[start..];
    let (mut depth, mut quote, mut escaped) = (0u32, false, false);
    for (end, byte) in bytes.iter().copied().enumerate() {
        if quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quote = false;
            }
        } else {
            match byte {
                b'"' => quote = true,
                b'{' => depth += 1,
                b'}' => {
                    depth = depth.checked_sub(1).ok_or(())?;
                    if depth == 0 {
                        return Parser::new(std::str::from_utf8(&bytes[..=end]).map_err(|_| ())?)
                            .parse();
                    }
                }
                _ => {}
            }
        }
    }
    Err(())
}
fn exact(object: &std::collections::BTreeMap<String, Json>, expected: &[&str]) -> Result<(), ()> {
    (object.len() == expected.len() && expected.iter().all(|key| object.contains_key(*key)))
        .then_some(())
        .ok_or(())
}
fn string<'a>(
    object: &'a std::collections::BTreeMap<String, Json>,
    key: &str,
) -> Result<&'a str, ()> {
    match object.get(key) {
        Some(Json::String(value)) => Ok(value),
        _ => Err(()),
    }
}
fn number(object: &std::collections::BTreeMap<String, Json>, key: &str) -> Result<u64, ()> {
    match object.get(key) {
        Some(Json::Number(value)) => Ok(*value),
        _ => Err(()),
    }
}
fn boolean(object: &std::collections::BTreeMap<String, Json>, key: &str) -> Result<bool, ()> {
    match object.get(key) {
        Some(Json::Bool(value)) => Ok(*value),
        _ => Err(()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Json {
    Null,
    Bool(bool),
    Number(u64),
    String(String),
    Array(Vec<Json>),
    Object(std::collections::BTreeMap<String, Json>),
}
struct Parser<'a> {
    input: &'a [u8],
    at: usize,
}
impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            at: 0,
        }
    }
    fn parse(mut self) -> Result<Json, ()> {
        let value = self.value(0)?;
        self.ws();
        (self.at == self.input.len()).then_some(value).ok_or(())
    }
    fn value(&mut self, depth: usize) -> Result<Json, ()> {
        if depth > 32 {
            return Err(());
        };
        self.ws();
        match self.peek() {
            Some(b'{') => self.object(depth + 1),
            Some(b'[') => self.array(depth + 1),
            Some(b'"') => self.string().map(Json::String),
            Some(b't') if self.word(b"true") => Ok(Json::Bool(true)),
            Some(b'f') if self.word(b"false") => Ok(Json::Bool(false)),
            Some(b'n') if self.word(b"null") => Ok(Json::Null),
            Some(b'0'..=b'9') => self.number().map(Json::Number),
            _ => Err(()),
        }
    }
    fn object(&mut self, depth: usize) -> Result<Json, ()> {
        self.take(b'{')?;
        let mut out = std::collections::BTreeMap::new();
        self.ws();
        if self.take_if(b'}') {
            return Ok(Json::Object(out));
        };
        loop {
            let key = self.string()?;
            self.ws();
            self.take(b':')?;
            if out.insert(key, self.value(depth)?).is_some() {
                return Err(());
            };
            self.ws();
            if self.take_if(b'}') {
                return Ok(Json::Object(out));
            };
            self.take(b',')?;
        }
    }
    fn array(&mut self, depth: usize) -> Result<Json, ()> {
        self.take(b'[')?;
        let mut out = Vec::new();
        self.ws();
        if self.take_if(b']') {
            return Ok(Json::Array(out));
        };
        loop {
            if out.len() == 512 {
                return Err(());
            };
            out.push(self.value(depth)?);
            self.ws();
            if self.take_if(b']') {
                return Ok(Json::Array(out));
            };
            self.take(b',')?;
        }
    }
    fn string(&mut self) -> Result<String, ()> {
        self.take(b'"')?;
        let mut out = String::new();
        loop {
            match self.next().ok_or(())? {
                b'"' => return Ok(out),
                b'\\' => match self.next().ok_or(())? {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\x08'),
                    b'f' => out.push('\x0c'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    _ => return Err(()),
                },
                byte if (0x20..0x80).contains(&byte) => out.push(byte as char),
                _ => return Err(()),
            }
        }
    }
    fn number(&mut self) -> Result<u64, ()> {
        let begin = self.at;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.at += 1
        }
        if self.at - begin > 1 && self.input[begin] == b'0' {
            return Err(());
        };
        std::str::from_utf8(&self.input[begin..self.at])
            .map_err(|_| ())?
            .parse()
            .map_err(|_| ())
    }
    fn word(&mut self, word: &[u8]) -> bool {
        if self.input.get(self.at..self.at + word.len()) == Some(word) {
            self.at += word.len();
            true
        } else {
            false
        }
    }
    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.at += 1
        }
    }
    fn take(&mut self, byte: u8) -> Result<(), ()> {
        self.ws();
        self.take_if(byte).then_some(()).ok_or(())
    }
    fn take_if(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn peek(&self) -> Option<u8> {
        self.input.get(self.at).copied()
    }
    fn next(&mut self) -> Option<u8> {
        let value = self.peek()?;
        self.at += 1;
        Some(value)
    }
}
