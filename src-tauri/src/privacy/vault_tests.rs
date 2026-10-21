use super::vault::*;
use std::time::{Duration, SystemTime};

fn key(byte: u8) -> StaticKey {
    StaticKey([byte; 32])
}

fn later() -> SystemTime {
    SystemTime::now() + Duration::from_secs(3600)
}

#[test]
fn sealed_bytes_open_with_the_same_key_and_context_and_hide_the_plaintext() {
    let secret = b"the quarterly numbers are in the blue folder";
    let sealed = seal(&[7; 32], secret, "shot.png").unwrap();
    assert!(is_sealed(&sealed));
    assert!(!sealed.windows(secret.len()).any(|w| w == secret));
    assert_eq!(open(&[7; 32], &sealed, "shot.png").unwrap(), secret);
}

#[test]
fn a_wrong_key_a_wrong_name_or_a_flipped_byte_does_not_open() {
    let sealed = seal(&[7; 32], b"payload", "a.png").unwrap();
    assert!(open(&[8; 32], &sealed, "a.png").is_err());
    assert!(open(&[7; 32], &sealed, "b.png").is_err());
    let mut tampered = sealed.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 1;
    assert!(open(&[7; 32], &tampered, "a.png").is_err());
    assert!(open(&[7; 32], b"plain text", "a.png").is_err());
}

#[test]
fn two_seals_of_the_same_bytes_differ() {
    let a = seal(&[1; 32], b"same", "x").unwrap();
    let b = seal(&[1; 32], b"same", "x").unwrap();
    assert_ne!(a, b);
}

#[test]
fn sealing_a_file_replaces_it_with_a_sealed_copy_that_reads_back() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("frame.png");
    std::fs::write(&path, b"\x89PNG pretend pixels").unwrap();
    let sealed = seal_file(&key(3), &path).unwrap();
    assert!(!path.exists());
    assert!(sealed.ends_with("frame.png.fndrenc"));
    assert!(is_sealed(&std::fs::read(&sealed).unwrap()));
    assert_eq!(
        read_sealed(&key(3), &sealed).unwrap(),
        b"\x89PNG pretend pixels"
    );
    assert!(read_sealed(&key(4), &sealed).is_err());
}

#[cfg(unix)]
#[test]
fn sealed_files_are_readable_by_the_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let sealed = write_sealed(&key(3), &dir.path().join("e.jsonl"), b"{}").unwrap();
    let mode = std::fs::metadata(sealed).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn the_sweep_seals_old_images_in_nested_folders_and_leaves_other_files_and_recent_ones() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("screenshots").join("2026-10-01");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("a.png"), b"a").unwrap();
    std::fs::write(nested.join("b.JPG"), b"b").unwrap();
    std::fs::write(nested.join("notes.txt"), b"notes").unwrap();
    std::fs::write(nested.join("c.png.fndrenc"), b"already").unwrap();

    let recent = seal_capture_images(&key(5), dir.path());
    assert_eq!(recent.sealed, 0);
    assert_eq!(recent.skipped_recent, 2);

    let report = seal_image_directory(&key(5), &dir.path().join("screenshots"), later());
    assert_eq!(report.sealed, 2);
    assert_eq!(report.already_sealed, 1);
    assert_eq!(report.failed, 0);
    assert!(!nested.join("a.png").exists());
    assert!(nested.join("a.png.fndrenc").exists());
    assert!(nested.join("b.JPG.fndrenc").exists());
    assert_eq!(std::fs::read(nested.join("notes.txt")).unwrap(), b"notes");

    let again = seal_image_directory(&key(5), &dir.path().join("screenshots"), later());
    assert_eq!(again.sealed, 0);
    assert_eq!(again.already_sealed, 3);
}

#[test]
fn a_missing_folder_is_an_empty_sweep() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        seal_capture_images(&key(1), dir.path()),
        SealReport::default()
    );
}

#[test]
fn filevault_output_is_read_without_guessing() {
    assert_eq!(
        parse_fdesetup_status("FileVault is On.\n"),
        FileVaultStatus::On
    );
    assert_eq!(
        parse_fdesetup_status("FileVault is Off.\n"),
        FileVaultStatus::Off
    );
    assert_eq!(
        parse_fdesetup_status("FileVault is On.\nEncryption in progress: Percent completed = 40"),
        FileVaultStatus::Unknown
    );
    assert_eq!(parse_fdesetup_status(""), FileVaultStatus::Unknown);
    assert_eq!(
        parse_fdesetup_status("Error: boom"),
        FileVaultStatus::Unknown
    );
}

#[test]
fn the_statement_never_claims_the_database_is_encrypted() {
    let statement = encryption_statement(FileVaultStatus::Off);
    assert!(statement
        .not_encrypted
        .iter()
        .any(|line| line.to_lowercase().contains("memory database")));
    assert!(!statement
        .encrypted
        .iter()
        .any(|line| line.to_lowercase().contains("database")));
    assert!(statement
        .encrypted
        .iter()
        .any(|line| line.contains("few screenshots")));
    assert!(statement
        .encrypted
        .iter()
        .any(|line| line.contains("Privacy activity")));
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "queries the host's FileVault status"]
fn reports_real_filevault_status() {
    eprintln!("FileVault status: {:?}", file_vault_status());
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "creates and deletes one item in the login Keychain"]
fn the_keychain_returns_the_same_key_twice_and_a_new_one_after_deletion() {
    let keychain = KeychainKey::named("com.fndr.vault.test", "run-once");
    let _ = keychain.delete();
    let first = keychain.key().unwrap();
    assert_eq!(first, keychain.key().unwrap());
    keychain.delete().unwrap();
    assert_ne!(first, keychain.key().unwrap());
    keychain.delete().unwrap();
}
