use crate::adb::PackageId;
use flate2::read::DeflateDecoder;
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
const MAX_APK_MANIFEST_BYTES: u64 = 10 * 1024 * 1024;

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
        binary_xml_manifest(
            &mut file,
            method,
            local_header,
            compressed,
            uncompressed,
            len,
        )
    })
}

fn binary_xml_manifest(
    file: &mut File,
    method: u16,
    local_header: u64,
    compressed_len: u64,
    uncompressed_len: u64,
    archive_len: u64,
) -> bool {
    if !(8..=MAX_APK_MANIFEST_BYTES).contains(&uncompressed_len)
        || file.seek(SeekFrom::Start(local_header)).is_err()
    {
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
        .checked_add(compressed_len)
        .is_none_or(|end| end > archive_len)
        || file.seek(SeekFrom::Start(data_start)).is_err()
    {
        return false;
    }
    let mut compressed = vec![0; compressed_len as usize];
    if file.read_exact(&mut compressed).is_err() {
        return false;
    }
    let mut manifest = Vec::with_capacity(uncompressed_len as usize);
    match method {
        0 if compressed_len == uncompressed_len => manifest = compressed,
        8 => {
            let mut decoder =
                DeflateDecoder::new(compressed.as_slice()).take(MAX_APK_MANIFEST_BYTES + 1);
            if decoder.read_to_end(&mut manifest).is_err() {
                return false;
            }
        }
        _ => return false,
    }
    if manifest.len() as u64 != uncompressed_len {
        return false;
    }
    binary_xml(&manifest)
}

fn binary_xml(manifest: &[u8]) -> bool {
    if manifest.len() < 8
        || u16::from_le_bytes([manifest[0], manifest[1]]) != ANDROID_BINARY_XML
        || u16::from_le_bytes([manifest[2], manifest[3]]) != 8
        || u32::from_le_bytes([manifest[4], manifest[5], manifest[6], manifest[7]]) as usize
            != manifest.len()
    {
        return false;
    }
    let mut offset = 8usize;
    let mut depth = 0usize;
    let mut root = false;
    let mut strings = None;
    while offset < manifest.len() {
        if manifest.len() - offset < 8 {
            return false;
        }
        let kind = u16::from_le_bytes([manifest[offset], manifest[offset + 1]]);
        let header_len = u16::from_le_bytes([manifest[offset + 2], manifest[offset + 3]]) as usize;
        let chunk_len = u32::from_le_bytes([
            manifest[offset + 4],
            manifest[offset + 5],
            manifest[offset + 6],
            manifest[offset + 7],
        ]) as usize;
        if header_len < 8 || chunk_len < header_len || chunk_len > manifest.len() - offset {
            return false;
        }
        let chunk = &manifest[offset..offset + chunk_len];
        match kind {
            0x0001 if header_len >= 28 => strings = Some(chunk),
            ANDROID_XML_START_ELEMENT if header_len >= 16 && chunk_len >= 36 => {
                if depth == 0
                    && !strings.is_some_and(|pool| {
                        manifest_name(
                            pool,
                            u32::from_le_bytes([chunk[20], chunk[21], chunk[22], chunk[23]]),
                        ) && package_attribute(pool, chunk)
                    })
                {
                    return false;
                }
                depth += 1;
                root = true;
            }
            ANDROID_XML_END_ELEMENT if header_len >= 16 && chunk_len >= 24 && depth > 0 => {
                depth -= 1
            }
            ANDROID_XML_END_ELEMENT => return false,
            _ => {}
        }
        offset += chunk_len;
    }
    root && depth == 0
}

fn package_attribute(pool: &[u8], node: &[u8]) -> bool {
    let start = 16 + u16::from_le_bytes([node[24], node[25]]) as usize;
    let size = u16::from_le_bytes([node[26], node[27]]) as usize;
    let count = u16::from_le_bytes([node[28], node[29]]) as usize;
    if size < 20
        || start
            .checked_add(size * count)
            .is_none_or(|end| end > node.len())
    {
        return false;
    }
    (0..count).any(|index| {
        let attribute = &node[start + index * size..start + (index + 1) * size];
        let namespace =
            u32::from_le_bytes([attribute[0], attribute[1], attribute[2], attribute[3]]);
        let name = u32::from_le_bytes([attribute[4], attribute[5], attribute[6], attribute[7]]);
        let raw = u32::from_le_bytes([attribute[8], attribute[9], attribute[10], attribute[11]]);
        let value = if raw != u32::MAX {
            raw
        } else if attribute[15] == 3 {
            u32::from_le_bytes([attribute[16], attribute[17], attribute[18], attribute[19]])
        } else {
            return false;
        };
        namespace == u32::MAX
            && string_value(pool, name).as_deref() == Some("package")
            && string_value(pool, value).is_some_and(|value| PackageId::parse(&value).is_ok())
    })
}

fn manifest_name(pool: &[u8], index: u32) -> bool {
    if pool.len() < 28 {
        return false;
    }
    let count = u32::from_le_bytes([pool[8], pool[9], pool[10], pool[11]]);
    let flags = u32::from_le_bytes([pool[16], pool[17], pool[18], pool[19]]);
    let strings_start = u32::from_le_bytes([pool[20], pool[21], pool[22], pool[23]]) as usize;
    let header_len = u16::from_le_bytes([pool[2], pool[3]]) as usize;
    if index >= count {
        return false;
    }
    let Some(offset_index) = header_len.checked_add(index as usize * 4) else {
        return false;
    };
    let Some(string_offset) = pool
        .get(offset_index..offset_index + 4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()) as usize)
    else {
        return false;
    };
    let Some(start) = strings_start.checked_add(string_offset) else {
        return false;
    };
    if flags & 0x100 != 0 {
        let Some((_, after_chars)) = encoded_length(pool, start) else {
            return false;
        };
        let Some((length, data_start)) = encoded_length(pool, after_chars) else {
            return false;
        };
        pool.get(data_start..data_start + length) == Some(b"manifest".as_slice())
            && pool.get(data_start + length) == Some(&0)
    } else {
        let Some(length) = pool
            .get(start..start + 2)
            .map(|bytes| u16::from_le_bytes(bytes.try_into().unwrap()) as usize)
        else {
            return false;
        };
        length == 8
            && pool.get(start + 2..start + 18)
                == Some(&[
                    b'm', 0, b'a', 0, b'n', 0, b'i', 0, b'f', 0, b'e', 0, b's', 0, b't', 0,
                ])
    }
}

fn string_value(pool: &[u8], index: u32) -> Option<String> {
    if pool.len() < 28 {
        return None;
    }
    let count = u32::from_le_bytes([pool[8], pool[9], pool[10], pool[11]]);
    let flags = u32::from_le_bytes([pool[16], pool[17], pool[18], pool[19]]);
    let strings_start = u32::from_le_bytes([pool[20], pool[21], pool[22], pool[23]]) as usize;
    let header_len = u16::from_le_bytes([pool[2], pool[3]]) as usize;
    if index >= count {
        return None;
    }
    let offset_index = header_len.checked_add(index as usize * 4)?;
    let string_offset =
        u32::from_le_bytes(pool.get(offset_index..offset_index + 4)?.try_into().ok()?) as usize;
    let start = strings_start.checked_add(string_offset)?;
    if flags & 0x100 != 0 {
        let (_, after_chars) = encoded_length(pool, start)?;
        let (length, data_start) = encoded_length(pool, after_chars)?;
        std::str::from_utf8(pool.get(data_start..data_start + length)?)
            .ok()
            .map(str::to_owned)
    } else {
        let length = u16::from_le_bytes(pool.get(start..start + 2)?.try_into().ok()?) as usize;
        let bytes = pool.get(start + 2..start + 2 + length * 2)?;
        String::from_utf16(
            &bytes
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect::<Vec<_>>(),
        )
        .ok()
    }
}

fn encoded_length(bytes: &[u8], start: usize) -> Option<(usize, usize)> {
    let first = *bytes.get(start)?;
    if first & 0x80 == 0 {
        Some((first as usize, start + 1))
    } else {
        Some((
            ((first as usize & 0x7f) << 8) | *bytes.get(start + 1)? as usize,
            start + 2,
        ))
    }
}
