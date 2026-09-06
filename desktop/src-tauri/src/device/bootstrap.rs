use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

const SHA256_HEX: usize = 64;

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
    path.is_file()
        && path
            .metadata()
            .is_ok_and(|metadata| (4..=100 * 1024 * 1024).contains(&metadata.len()))
        && File::open(path)
            .and_then(|mut file| {
                let mut magic = [0; 4];
                file.read_exact(&mut magic).map(|_| magic)
            })
            .is_ok_and(|magic| magic == *b"PK\x03\x04")
        && signing_sha256.len() == SHA256_HEX
        && signing_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
