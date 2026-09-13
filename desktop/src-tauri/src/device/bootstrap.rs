use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

const SHA256_HEX: usize = 64;
const APK_MANIFEST: &[u8] = b"AndroidManifest.xml";
const ZIP_CENTRAL_DIRECTORY: &[u8; 4] = b"PK\x01\x02";
const ZIP_END_OF_CENTRAL_DIRECTORY: &[u8; 4] = b"PK\x05\x06";
const ZIP_EOCD_MAX_SIZE: u64 = 22 + u16::MAX as u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherArtifact {
    path: PathBuf,
    signing_sha256: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LauncherArtifactError {
    Invalid,
}

impl LauncherArtifact {
    pub fn from_path(
        path: impl AsRef<Path>,
        signing_sha256: impl Into<String>,
    ) -> Result<Self, LauncherArtifactError> {
        let path = path.as_ref();
        let signing_sha256 = signing_sha256.into();
        if !valid(path, &signing_sha256) {
            return Err(LauncherArtifactError::Invalid);
        }
        Ok(Self {
            path: path.into(),
            signing_sha256,
        })
    }
    pub(crate) fn validate(&self) -> Result<(), LauncherArtifactError> {
        valid(&self.path, &self.signing_sha256)
            .then_some(())
            .ok_or(LauncherArtifactError::Invalid)
    }
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
    pub(crate) fn signing_sha256(&self) -> &str {
        &self.signing_sha256
    }
}
fn valid(path: &Path, signing_sha256: &str) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    let len = metadata.len();
    path.is_file()
        && (4..=100 * 1024 * 1024).contains(&len)
        && File::open(path)
            .and_then(|mut file| {
                let mut magic = [0; 4];
                file.read_exact(&mut magic).map(|_| magic)
            })
            .is_ok_and(|magic| magic == *b"PK\x03\x04")
        && apk_zip(path, len)
        && signing_sha256.len() == SHA256_HEX
        && signing_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn apk_zip(path: &Path, len: u64) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let tail_len = len.min(ZIP_EOCD_MAX_SIZE) as usize;
    let tail_start = len - tail_len as u64;
    if file.seek(SeekFrom::Start(tail_start)).is_err() {
        return false;
    }
    let mut tail = vec![0; tail_len];
    if file.read_exact(&mut tail).is_err() {
        return false;
    }
    let Some(eocd) = (0..=tail_len.saturating_sub(22)).rev().find(|&index| {
        tail[index..].starts_with(ZIP_END_OF_CENTRAL_DIRECTORY)
            && index + 22 + u16::from_le_bytes([tail[index + 20], tail[index + 21]]) as usize
                == tail_len
    }) else {
        return false;
    };
    let entries = u16::from_le_bytes([tail[eocd + 10], tail[eocd + 11]]);
    let directory_len = u32::from_le_bytes([
        tail[eocd + 12],
        tail[eocd + 13],
        tail[eocd + 14],
        tail[eocd + 15],
    ]) as u64;
    let directory_start = u32::from_le_bytes([
        tail[eocd + 16],
        tail[eocd + 17],
        tail[eocd + 18],
        tail[eocd + 19],
    ]) as u64;
    let eocd_start = tail_start + eocd as u64;
    let Some(directory_end) = directory_start.checked_add(directory_len) else {
        return false;
    };
    if entries == 0
        || directory_end > eocd_start
        || file.seek(SeekFrom::Start(directory_start)).is_err()
    {
        return false;
    }
    let mut remaining = directory_len;
    let mut manifest = false;
    for _ in 0..entries {
        if remaining < 46 {
            return false;
        }
        let mut header = [0; 46];
        if file.read_exact(&mut header).is_err() || &header[..4] != ZIP_CENTRAL_DIRECTORY {
            return false;
        }
        let name_len = u16::from_le_bytes([header[28], header[29]]) as u64;
        let extra_len = u16::from_le_bytes([header[30], header[31]]) as u64;
        let comment_len = u16::from_le_bytes([header[32], header[33]]) as u64;
        let Some(record_len) = 46u64
            .checked_add(name_len)
            .and_then(|len| len.checked_add(extra_len))
            .and_then(|len| len.checked_add(comment_len))
        else {
            return false;
        };
        if record_len > remaining {
            return false;
        }
        let mut name = vec![0; name_len as usize];
        if file.read_exact(&mut name).is_err() {
            return false;
        }
        manifest |= name == APK_MANIFEST;
        if file
            .seek(SeekFrom::Current((extra_len + comment_len) as i64))
            .is_err()
        {
            return false;
        }
        remaining -= record_len;
    }
    manifest
}
