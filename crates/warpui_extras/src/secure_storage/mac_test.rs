use tempfile::TempDir;

use super::SecureStorage;
use crate::secure_storage::{Error, SecureStorage as Trait};

#[test]
fn test_encrypt_decrypt_round_trip() {
    let inputs: Vec<String> = vec![
        "freckles grain uncaring strict stumbling reappear".into(),
        "".into(),
        "{".into(),
        "\'".into(),
        "\"".into(),
        "{\"test\"}".into(),
        "{\"id_token\":\"abc\",\"refresh_token\":\"def\",\"expiration_time\":\"2099-01-01T00:00:00Z\"}"
            .into(),
        // Larger payload to exercise the AEAD with non-trivial sizes.
        "x".repeat(8192),
    ];

    for input in &inputs {
        let encrypted = SecureStorage::encrypt(input).expect("encrypt");
        let decrypted = SecureStorage::decrypt(&encrypted).expect("decrypt");
        assert_eq!(&decrypted, input, "round trip for {input:?}");
    }
}

#[test]
fn test_encrypt_uses_fresh_nonce_each_call() {
    // Same plaintext must yield distinct ciphertexts; otherwise we are leaking
    // key reuse on identical inputs.
    let value = "same plaintext";
    let a = SecureStorage::encrypt(value).expect("encrypt a");
    let b = SecureStorage::encrypt(value).expect("encrypt b");
    assert_ne!(a, b);
}

#[test]
fn test_decrypt_truncated_input_errors() {
    assert!(SecureStorage::decrypt(&[]).is_err());
    assert!(SecureStorage::decrypt(&[0u8; 5]).is_err());
}

fn fresh_storage() -> (TempDir, SecureStorage) {
    let dir = TempDir::new().expect("tempdir");
    let storage = SecureStorage::new_with_path("test.service", dir.path().to_path_buf());
    (dir, storage)
}

#[test]
fn test_write_read_remove_round_trip() {
    let (_dir, storage) = fresh_storage();

    storage.write_value("k1", "v1").expect("write");
    assert_eq!(storage.read_value("k1").unwrap(), "v1");

    storage.write_value("k1", "v2").expect("overwrite");
    assert_eq!(storage.read_value("k1").unwrap(), "v2");

    storage.remove_value("k1").expect("remove");
    match storage.read_value("k1") {
        Err(Error::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[test]
fn test_read_missing_key_returns_not_found() {
    let (_dir, storage) = fresh_storage();
    match storage.read_value("nope") {
        Err(Error::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[test]
fn test_remove_missing_key_returns_not_found() {
    let (_dir, storage) = fresh_storage();
    match storage.remove_value("nope") {
        Err(Error::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[test]
fn test_creates_storage_dir_if_missing() {
    let parent = TempDir::new().expect("tempdir");
    let nested = parent.path().join("does/not/exist/yet");
    assert!(!nested.exists());

    let storage = SecureStorage::new_with_path("test.service", nested.clone());
    storage
        .write_value("k", "v")
        .expect("write should create dir");
    assert!(nested.is_dir());
    assert_eq!(storage.read_value("k").unwrap(), "v");
}

#[test]
fn test_file_permissions_are_0600() {
    use std::os::unix::fs::PermissionsExt;

    let (dir, storage) = fresh_storage();
    storage.write_value("k", "v").expect("write");
    let path = dir.path().join("test.service-k");
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "expected 0600 mode, got {mode:o}");
}
