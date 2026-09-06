use std::path::{Path, PathBuf};

const SHA256_HEX: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherArtifact {
    pub path: PathBuf,
    pub signing_sha256: String,
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
        if !path.is_file()
            || signing_sha256.len() != SHA256_HEX
            || !signing_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(LauncherArtifactError::Invalid);
        }
        Ok(Self {
            path: path.into(),
            signing_sha256,
        })
    }
}
