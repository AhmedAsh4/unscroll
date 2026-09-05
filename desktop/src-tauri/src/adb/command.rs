use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    InvalidSerial,
    InvalidPackage,
    InvalidUser,
    InvalidComponent,
    InvalidAppOp,
    InvalidDestination,
    InvalidFingerprint,
    InvalidStreamId,
}
fn safe(value: &str, max: usize, allowed: impl Fn(u8, usize) -> bool) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| allowed(byte, index))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Serial(String);
impl Serial {
    pub fn parse(value: &str) -> Result<Self, ValidationError> {
        if safe(value, 512, |byte, _| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
        }) && !value.starts_with('-')
        {
            Ok(Self(value.into()))
        } else {
            Err(ValidationError::InvalidSerial)
        }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageId(String);
impl PackageId {
    pub fn parse(value: &str) -> Result<Self, ValidationError> {
        let valid_part = |part: &str| {
            safe(part, 255, |byte, index| {
                byte == b'_'
                    || (byte == b'$' && index > 0)
                    || (byte.is_ascii_alphanumeric()
                        && (index > 0 || byte.is_ascii_alphabetic() || byte == b'_'))
            })
        };
        if value.len() <= 255 && value.split('.').count() >= 2 && value.split('.').all(valid_part) {
            Ok(Self(value.into()))
        } else {
            Err(ValidationError::InvalidPackage)
        }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserId;
impl UserId {
    pub fn parse(value: u32) -> Result<Self, ValidationError> {
        (value == 0)
            .then_some(Self)
            .ok_or(ValidationError::InvalidUser)
    }
    pub fn get(self) -> u32 {
        0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component(String);
impl Component {
    pub fn parse(value: &str) -> Result<Self, ValidationError> {
        let Some((package, class)) = value.split_once('/') else {
            return Err(ValidationError::InvalidComponent);
        };
        if PackageId::parse(package).is_ok()
            && safe(class, 512, |byte, _| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'$')
            })
        {
            Ok(Self(value.into()))
        } else {
            Err(ValidationError::InvalidComponent)
        }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppOp(String);
impl AppOp {
    pub fn parse(value: &str) -> Result<Self, ValidationError> {
        safe(value, 128, |byte, _| {
            byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'
        })
        .then(|| Self(value.into()))
        .ok_or(ValidationError::InvalidAppOp)
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppOpMode {
    Default,
    Allow,
    Ignore,
}
impl AppOpMode {
    pub fn parse(value: &str) -> Result<Self, ValidationError> {
        match value {
            "default" => Ok(Self::Default),
            "allow" => Ok(Self::Allow),
            "ignore" => Ok(Self::Ignore),
            _ => Err(ValidationError::InvalidAppOp),
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Allow => "allow",
            Self::Ignore => "ignore",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint(String);
impl Fingerprint {
    pub fn parse(value: &str) -> Result<Self, ValidationError> {
        safe(value, 512, |byte, _| {
            byte.is_ascii_graphic() || byte == b' '
        })
        .then(|| Self(value.into()))
        .ok_or(ValidationError::InvalidFingerprint)
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamId(String);
impl StreamId {
    pub fn parse(value: &str) -> Result<Self, ValidationError> {
        (value.len() == 32
            && value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()))
        .then(|| Self(value.into()))
        .ok_or(ValidationError::InvalidStreamId)
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destination(String);
impl Destination {
    pub fn parse(value: &str) -> Result<Self, ValidationError> {
        let prefix = "/sdcard/Documents/Unscroll/";
        if value.starts_with(prefix)
            && value.len() <= 512
            && value[prefix.len()..].split('/').all(|part| {
                safe(part, 128, |byte, _| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
                })
            })
            && !value.contains("..")
        {
            Ok(Self(value.into()))
        } else {
            Err(ValidationError::InvalidDestination)
        }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Property {
    SdkInt,
    Model,
    Fingerprint,
}
impl Property {
    fn as_str(self) -> &'static str {
        match self {
            Self::SdkInt => "ro.build.version.sdk",
            Self::Model => "ro.product.model",
            Self::Fingerprint => "ro.build.fingerprint",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeOperation {
    Health,
    DeviceFacts {
        device_serial: Serial,
    },
    Catalog {
        device_serial: Serial,
        fingerprint: Fingerprint,
    },
    Icon {
        package: PackageId,
        component: Component,
    },
    ReadEnvelope {
        device_serial: Serial,
        fingerprint: Fingerprint,
    },
    ReadIcon(StreamId),
}
impl BridgeOperation {
    fn arguments(&self) -> Vec<String> {
        const URI: &str = "content://org.unscroll.launcher.bridge";
        let request = match self {
            Self::Health => Some("{\"operation\":\"health\",\"args\":{}}".into()),
            Self::DeviceFacts { device_serial } => Some(format!("{{\"operation\":\"device_facts\",\"args\":{{\"serial\":\"{}\"}}}}", device_serial.as_str())),
            Self::Catalog { device_serial, fingerprint } => Some(format!("{{\"operation\":\"catalog_page\",\"args\":{{\"page_size\":1,\"device_binding\":{{\"serial\":\"{}\",\"fingerprint\":\"{}\",\"user_id\":0}}}}}}", device_serial.as_str(), fingerprint.as_str())),
            Self::Icon { package, component } => {
                let (_, activity) = component.as_str().split_once('/').expect("validated component");
                Some(format!("{{\"operation\":\"icon_stream\",\"args\":{{\"package_id\":\"{}\",\"activity_name\":\"{}\",\"user_id\":0}}}}", package.as_str(), activity))
            }
            Self::ReadEnvelope { device_serial, fingerprint } => Some(format!("{{\"operation\":\"read_envelope\",\"args\":{{\"device_binding\":{{\"serial\":\"{}\",\"fingerprint\":\"{}\",\"user_id\":0}}}}}}", device_serial.as_str(), fingerprint.as_str())),
            Self::ReadIcon(stream) => return vec!["content".into(), "read".into(), "--uri".into(), format!("{URI}/recovery-v1/icon/{}", stream.as_str())],
        };
        vec![
            "content".into(),
            "call".into(),
            "--uri".into(),
            URI.into(),
            "--method".into(),
            "recovery-v1".into(),
            "--extra".into(),
            format!("request:s:{}", request.expect("call request")),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceOperation {
    GetProperty(Property),
    PackageInfo {
        package: PackageId,
        user: UserId,
    },
    PackageState {
        package: PackageId,
        user: UserId,
    },
    AppOpGet {
        package: PackageId,
        user: UserId,
        app_op: AppOp,
    },
    AppOpSet {
        package: PackageId,
        user: UserId,
        app_op: AppOp,
        mode: AppOpMode,
    },
    Suspend {
        package: PackageId,
        user: UserId,
        suspended: bool,
    },
    HomeResolve,
    HomeSelect(Component),
    HomeChooser,
    Bridge(BridgeOperation),
    ComponentInfo(Component),
    ReadDestination(Destination),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdbCommand {
    StartServer,
    StopServer,
    Devices,
    Device {
        serial: Serial,
        operation: DeviceOperation,
    },
}
impl AdbCommand {
    pub fn arguments(&self) -> Vec<String> {
        match self {
            Self::StartServer => vec!["start-server".into()],
            Self::StopServer => vec!["kill-server".into()],
            Self::Devices => vec!["devices".into(), "-l".into()],
            Self::Device { serial, operation } => {
                let mut args = vec!["-s".into(), serial.0.clone(), "shell".into()];
                match operation {
                    DeviceOperation::GetProperty(property) => {
                        args.extend(["getprop".into(), property.as_str().into()])
                    }
                    DeviceOperation::PackageInfo { package, user } => args.extend([
                        "cmd".into(),
                        "package".into(),
                        "list".into(),
                        "packages".into(),
                        "--user".into(),
                        user.get().to_string(),
                        package.0.clone(),
                    ]),
                    DeviceOperation::PackageState { package, user } => args.extend([
                        "dumpsys".into(),
                        "package".into(),
                        "--user".into(),
                        user.get().to_string(),
                        package.0.clone(),
                    ]),
                    DeviceOperation::AppOpGet {
                        package,
                        user,
                        app_op,
                    } => args.extend([
                        "appops".into(),
                        "get".into(),
                        "--user".into(),
                        user.get().to_string(),
                        package.0.clone(),
                        app_op.0.clone(),
                    ]),
                    DeviceOperation::AppOpSet {
                        package,
                        user,
                        app_op,
                        mode,
                    } => args.extend([
                        "appops".into(),
                        "set".into(),
                        "--user".into(),
                        user.get().to_string(),
                        package.0.clone(),
                        app_op.0.clone(),
                        mode.as_str().into(),
                    ]),
                    DeviceOperation::Suspend {
                        package,
                        user,
                        suspended,
                    } => args.extend([
                        "cmd".into(),
                        "package".into(),
                        if *suspended {
                            "suspend".into()
                        } else {
                            "unsuspend".into()
                        },
                        "--user".into(),
                        user.get().to_string(),
                        package.0.clone(),
                    ]),
                    DeviceOperation::HomeResolve => args.extend([
                        "cmd".into(),
                        "package".into(),
                        "resolve-activity".into(),
                        "--brief".into(),
                        "--components".into(),
                        "--user".into(),
                        "0".into(),
                        "-a".into(),
                        "android.intent.action.MAIN".into(),
                        "-c".into(),
                        "android.intent.category.HOME".into(),
                    ]),
                    DeviceOperation::HomeSelect(package) => args.extend([
                        "cmd".into(),
                        "package".into(),
                        "set-home-activity".into(),
                        "--user".into(),
                        "0".into(),
                        package.0.clone(),
                    ]),
                    DeviceOperation::HomeChooser => args.extend([
                        "am".into(),
                        "start".into(),
                        "-W".into(),
                        "-a".into(),
                        "android.intent.action.MAIN".into(),
                        "-c".into(),
                        "android.intent.category.HOME".into(),
                    ]),
                    DeviceOperation::Bridge(operation) => args.extend(operation.arguments()),
                    DeviceOperation::ComponentInfo(component) => args.extend([
                        "cmd".into(),
                        "package".into(),
                        "resolve-activity".into(),
                        "--brief".into(),
                        "--components".into(),
                        "--user".into(),
                        "0".into(),
                        "-n".into(),
                        component.0.clone(),
                    ]),
                    DeviceOperation::ReadDestination(destination) => {
                        args.extend(["cat".into(), destination.0.clone()])
                    }
                };
                args
            }
        }
    }
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(if matches!(self, Self::StartServer | Self::StopServer) {
            15
        } else {
            10
        })
    }
    pub fn class(&self) -> &'static str {
        match self {
            Self::StartServer => "server-start",
            Self::StopServer => "server-stop",
            Self::Devices => "device-discovery",
            Self::Device {
                operation: DeviceOperation::GetProperty(_),
                ..
            } => "device-property",
            Self::Device {
                operation: DeviceOperation::PackageInfo { .. },
                ..
            } => "package-info",
            Self::Device {
                operation: DeviceOperation::PackageState { .. },
                ..
            } => "package-state",
            Self::Device {
                operation: DeviceOperation::AppOpGet { .. },
                ..
            } => "app-op-get",
            Self::Device {
                operation: DeviceOperation::AppOpSet { .. },
                ..
            } => "app-op-set",
            Self::Device {
                operation: DeviceOperation::Suspend { .. },
                ..
            } => "package-suspension",
            Self::Device {
                operation: DeviceOperation::HomeResolve,
                ..
            } => "home-resolve",
            Self::Device {
                operation: DeviceOperation::HomeSelect(_),
                ..
            } => "home-select",
            Self::Device {
                operation: DeviceOperation::HomeChooser,
                ..
            } => "home-chooser",
            Self::Device {
                operation: DeviceOperation::Bridge(_),
                ..
            } => "bridge",
            Self::Device {
                operation: DeviceOperation::ComponentInfo(_),
                ..
            } => "component-info",
            Self::Device {
                operation: DeviceOperation::ReadDestination(_),
                ..
            } => "destination-read",
        }
    }
    pub fn serial(&self) -> Option<&Serial> {
        match self {
            Self::Device { serial, .. } => Some(serial),
            _ => None,
        }
    }
}
