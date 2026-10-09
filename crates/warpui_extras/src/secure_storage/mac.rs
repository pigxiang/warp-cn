//! File-based implementation of [`SecureStorage`] for macOS.
//!
//! The warp-cn fork ships ad-hoc signed binaries on macOS, so the Keychain
//! ACL trust list is keyed by CDHash with no stable designated requirement
//! across releases. Each upgrade therefore triggers a Keychain prompt for
//! every previously stored item. To avoid that, secrets are persisted as
//! AES-256-GCM encrypted files under the application's state directory,
//! mirroring the Linux disk fallback.

use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

use anyhow::{Context, anyhow};
use rand::RngCore;
use ring::aead;

use super::Error;

pub struct SecureStorage {
    service_name: String,
    storage_dir: PathBuf,
}

impl SecureStorage {
    pub fn new_with_path(service_name: &str, storage_dir: PathBuf) -> Self {
        Self {
            service_name: service_name.to_owned(),
            storage_dir,
        }
    }

    fn storage_file(&self, key: &str) -> PathBuf {
        self.storage_dir
            .join(format!("{}-{key}", self.service_name))
    }

    fn aead_key() -> Result<aead::LessSafeKey, Error> {
        // Inconspicuous bytes; same scheme as linux.rs::encryption_key so the
        // helper can later be extracted into a shared module.
        let mut bytes = Vec::from("https://releases.warp.dev/channel_versions.json");
        bytes.resize(aead::AES_256_GCM.key_len(), 0);
        let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, &bytes)
            .map_err(|_| Error::Unknown(anyhow!("invalid encryption key")))?;
        Ok(aead::LessSafeKey::new(unbound))
    }

    fn encrypt(value: &str) -> Result<Vec<u8>, Error> {
        let key = Self::aead_key()?;
        let mut nonce_bytes = [0u8; aead::NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = aead::Nonce::assume_unique_for_key(nonce_bytes);

        let mut data = value.as_bytes().to_vec();
        key.seal_in_place_append_tag(nonce, aead::Aad::empty(), &mut data)
            .map_err(Into::<Error>::into)
            .context("encryption failed")?;

        let mut out = Vec::with_capacity(aead::NONCE_LEN + data.len());
        out.extend_from_slice(&nonce_bytes);
        out.append(&mut data);
        Ok(out)
    }

    fn decrypt(blob: &[u8]) -> Result<String, Error> {
        if blob.len() < aead::NONCE_LEN + 1 {
            return Err(Error::Unknown(anyhow!("ciphertext too short")));
        }
        let key = Self::aead_key()?;
        let nonce = aead::Nonce::try_assume_unique_for_key(&blob[..aead::NONCE_LEN])
            .map_err(Into::<Error>::into)
            .context("invalid nonce")?;

        let mut data = blob[aead::NONCE_LEN..].to_vec();
        let len = key
            .open_in_place(nonce, aead::Aad::empty(), &mut data)
            .map_err(Into::<Error>::into)
            .context("decryption failed")?
            .len();
        data.truncate(len);
        String::from_utf8(data).map_err(|err| Error::DecodeError(err.utf8_error()))
    }
}

impl super::SecureStorage for SecureStorage {
    fn write_value(&self, key: &str, value: &str) -> Result<(), Error> {
        std::fs::create_dir_all(&self.storage_dir).map_err(|err| Error::Unknown(err.into()))?;
        let bytes = Self::encrypt(value)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(self.storage_file(key))
            .map_err(|err| Error::Unknown(err.into()))?;
        file.write_all(&bytes)
            .map_err(|err| Error::Unknown(err.into()))
    }

    fn read_value(&self, key: &str) -> Result<String, Error> {
        let bytes = std::fs::read(self.storage_file(key)).map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => Error::NotFound,
            _ => Error::Unknown(err.into()),
        })?;
        Self::decrypt(&bytes)
    }

    fn remove_value(&self, key: &str) -> Result<(), Error> {
        std::fs::remove_file(self.storage_file(key)).map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => Error::NotFound,
            _ => Error::Unknown(err.into()),
        })
    }
}

impl From<ring::error::Unspecified> for Error {
    fn from(value: ring::error::Unspecified) -> Self {
        Error::Unknown(anyhow!(value))
    }
}

#[cfg(test)]
#[path = "mac_test.rs"]
mod tests;
