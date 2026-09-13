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
const ZIP_LOCAL_FILE: &[u8; 4] = b"PK\x03\x04";
const ANDROID_BINARY_XML: u16 = 0x0003;
const ANDROID_XML_START_ELEMENT: u16 = 0x0102;
const ANDROID_XML_END_ELEMENT: u16 = 0x0103;

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
    let mut manifest = None;
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
        if name == APK_MANIFEST {
            manifest = Some((
                u16::from_le_bytes([header[10], header[11]]),
                u32::from_le_bytes([header[20], header[21], header[22], header[23]]) as u64,
                u32::from_le_bytes([header[24], header[25], header[26], header[27]]) as u64,
                u32::from_le_bytes([header[42], header[43], header[44], header[45]]) as u64,
            ));
        }
        if file
            .seek(SeekFrom::Current((extra_len + comment_len) as i64))
            .is_err()
        {
            return false;
        }
        remaining -= record_len;
    }
    manifest.is_some_and(|(method, compressed, uncompressed, local_header)| {
        method == 0
            && compressed == uncompressed
            && binary_xml_manifest(&mut file, local_header, uncompressed, len)
    })
}

fn binary_xml_manifest(
    file: &mut File,
    local_header: u64,
    manifest_len: u64,
    archive_len: u64,
) -> bool {
    if manifest_len < 8 || file.seek(SeekFrom::Start(local_header)).is_err() {
        return false;
    }
    let mut local = [0; 30];
    if file.read_exact(&mut local).is_err() || &local[..4] != ZIP_LOCAL_FILE {
        return false;
    }
    let name_len = u16::from_le_bytes([local[26], local[27]]) as u64;
    let extra_len = u16::from_le_bytes([local[28], local[29]]) as u64;
    let Some(data_start) = local_header
        .checked_add(30)
        .and_then(|offset| offset.checked_add(name_len))
        .and_then(|offset| offset.checked_add(extra_len))
    else {
        return false;
    };
    if data_start
        .checked_add(manifest_len)
        .is_none_or(|end| end > archive_len)
        || file.seek(SeekFrom::Start(data_start)).is_err()
    {
        return false;
    }
    let mut root = [0; 8];
    if file.read_exact(&mut root).is_err()
        || u16::from_le_bytes([root[0], root[1]]) != ANDROID_BINARY_XML
        || u16::from_le_bytes([root[2], root[3]]) != 8
        || u32::from_le_bytes([root[4], root[5], root[6], root[7]]) as u64 != manifest_len
    {
        return false;
    }
    let mut remaining = manifest_len - 8;
    let mut depth = 0usize;
    let mut has_element = false;
    while remaining > 0 {
        if remaining < 8 {
            return false;
        }
        let mut chunk = [0; 8];
        if file.read_exact(&mut chunk).is_err() {
            return false;
        }
        let kind = u16::from_le_bytes([chunk[0], chunk[1]]);
        let header_len = u16::from_le_bytes([chunk[2], chunk[3]]) as u64;
        let chunk_len = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]) as u64;
        if header_len < 8 || chunk_len < header_len || chunk_len > remaining {
            return false;
        }
        match kind {
            ANDROID_XML_START_ELEMENT if header_len >= 16 && chunk_len >= 36 => {
                depth += 1;
                has_element = true;
            }
            ANDROID_XML_END_ELEMENT if header_len >= 16 && chunk_len >= 24 && depth > 0 => {
                depth -= 1
            }
            ANDROID_XML_END_ELEMENT => return false,
            _ => {}
        }
        if file
            .seek(SeekFrom::Current((chunk_len - 8) as i64))
            .is_err()
        {
            return false;
        }
        remaining -= chunk_len;
    }
    has_element && depth == 0
}
