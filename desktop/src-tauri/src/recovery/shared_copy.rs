use crate::adb::Destination;
use std::io;

pub const SHARED_RECOVERY_PATH: &str = "/sdcard/Documents/Unscroll/recovery-v1.json";
pub const MAX_SHARED_COPY_BYTES: usize = 65_536;

pub fn bounded_read(value: &[u8]) -> io::Result<&[u8]> {
    (value.len() <= MAX_SHARED_COPY_BYTES)
        .then_some(value)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "shared recovery copy exceeds limit",
            )
        })
}

pub fn destination() -> Destination {
    Destination::parse(SHARED_RECOVERY_PATH).expect("fixed shared recovery path")
}
