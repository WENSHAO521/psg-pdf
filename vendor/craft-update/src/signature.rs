//! PSG patch: releases signed with the developer's own Ed25519 key instead of a code-signing
//! certificate.
//!
//! The release workflow signs the exact bytes of `SHA256SUMS.txt` and publishes the 64-byte
//! signature as `SHA256SUMS.txt.sig`. An app with [`App::update_key`] downloads a file only with
//! the checksum that signed list gives it, and keeps the list and its signature beside the
//! download (`<file>.sums`, `<file>.sums.sig`) so the step that installs it, later and possibly in
//! another process, checks the file again rather than trusting the download folder.
//!
//! Everything read from the network or the download folder is untrusted: sizes are capped, and a
//! key, signature or list that doesn't parse is refused, never skipped.

use std::path::{Path, PathBuf};

use crate::app::App;
use crate::release;

/// The signature's file name beside a release's checksum list.
pub const SIG_SUFFIX: &str = ".sig";
/// A checksum list bigger than this is refused (the updater's own cap for small files).
pub const MAX_SUMS: usize = 4 * 1024 * 1024;
const SIG_LEN: usize = 64;

/// The app's release key: `Ok(None)` when it has none (the upstream checks apply), an error when
/// it has one that isn't 32 bytes of hex, so a damaged build refuses updates instead of
/// skipping the check.
pub fn key(app: &App) -> Result<Option<[u8; 32]>, String> {
    let Some(text) = app.update_key.map(str::trim).filter(|t| !t.is_empty()) else { return Ok(None) };
    parse_hex_key(text).map(Some).ok_or_else(|| format!("{}'s update key is damaged, so it doesn't install updates", app.name))
}

fn parse_hex_key(text: &str) -> Option<[u8; 32]> {
    let bytes = text.as_bytes();
    if bytes.len() != 64 {
        return None;
    }
    let mut key = [0u8; 32];
    for (out, pair) in key.iter_mut().zip(bytes.chunks_exact(2)) {
        let hi = char::from(*pair.first()?).to_digit(16)?;
        let lo = char::from(*pair.get(1)?).to_digit(16)?;
        *out = u8::try_from(hi * 16 + lo).ok()?;
    }
    Some(key)
}

/// Whether `signature` is `key`'s Ed25519 signature of `message`.
pub fn verify(key: &[u8; 32], message: &[u8], signature: &[u8]) -> bool {
    signature.len() == SIG_LEN && ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, key).verify(message, signature).is_ok()
}

/// The checksum that a list signed by `key` gives `name`.
pub fn signed_checksum(app: &App, key: &[u8; 32], sums: &[u8], signature: &[u8], name: &str) -> Result<String, String> {
    if sums.len() > MAX_SUMS || !verify(key, sums, signature) {
        return Err(format!("the release's checksums aren't signed by {}'s developer, so nothing was installed", app.name));
    }
    let text = std::str::from_utf8(sums).map_err(|_| "the release's signed checksums aren't text".to_string())?;
    release::parse_sha256sums(text)
        .into_iter()
        .find(|(_, n)| n == name)
        .map(|(hash, _)| hash)
        .ok_or_else(|| format!("the release's signed checksums don't list {name}, so it wasn't installed"))
}

/// Where the list and the signature that vouch for `file` are kept.
fn beside(file: &Path) -> (PathBuf, PathBuf) {
    let mut sums = file.as_os_str().to_owned();
    sums.push(".sums");
    let mut sig = sums.clone();
    sig.push(SIG_SUFFIX);
    (PathBuf::from(sums), PathBuf::from(sig))
}

/// Keeps the signed list and its signature beside `file` for [`verify_download`].
pub fn keep_beside(file: &Path, sums: &[u8], signature: &[u8]) -> Result<(), String> {
    let (sums_path, sig_path) = beside(file);
    std::fs::write(&sums_path, sums)
        .and_then(|()| std::fs::write(&sig_path, signature))
        .map_err(|e| format!("couldn't keep the release's signed checksums ({e})"))
}

/// Removes what [`keep_beside`] wrote.
pub fn forget_beside(file: &Path) {
    let (sums, sig) = beside(file);
    let _ = std::fs::remove_file(sums);
    let _ = std::fs::remove_file(sig);
}

/// Checks `file` again against the signed list kept beside it. Does nothing for an app without a
/// key. `name` is the release's name for the file (the download keeps it).
pub fn verify_download(app: &App, file: &Path) -> Result<(), String> {
    let Some(key) = key(app)? else { return Ok(()) };
    let name = file.file_name().and_then(|n| n.to_str()).ok_or_else(|| format!("{} has no usable file name", file.display()))?;
    let (sums_path, sig_path) = beside(file);
    let read = |path: &Path, max: usize| -> Result<Vec<u8>, String> {
        let len = std::fs::metadata(path).map_err(|e| format!("the signed checksums for {name} are missing ({e})"))?.len();
        if len > max as u64 {
            return Err(format!("the signed checksums for {name} are too large"));
        }
        std::fs::read(path).map_err(|e| format!("couldn't read the signed checksums for {name} ({e})"))
    };
    let sums = read(&sums_path, MAX_SUMS)?;
    let signature = read(&sig_path, SIG_LEN)?;
    let want = signed_checksum(app, &key, &sums, &signature, name)?;
    let have = crate::http::sha256_file(file)?;
    if have.eq_ignore_ascii_case(&want) { Ok(()) } else { Err(format!("{name} changed after it was downloaded, so it wasn't installed")) }
}

#[cfg(test)]
mod tests {
    use ring::signature::{Ed25519KeyPair, KeyPair};

    use super::*;

    fn pair() -> Ed25519KeyPair {
        Ed25519KeyPair::from_seed_unchecked(&[7u8; 32]).unwrap()
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn app_with(key: &'static str) -> App {
        App { update_key: Some(key), ..App::new("psg-pdf", "PSG PDF", "PDFs", "WENSHAO521/psg-pdf") }
    }

    fn leaked_key() -> &'static str {
        Box::leak(hex(pair().public_key().as_ref()).into_boxed_str())
    }

    #[test]
    fn no_key_or_a_blank_one_keeps_the_upstream_checks() {
        assert_eq!(key(&App::new("a", "A", "", "o/a")), Ok(None));
        assert_eq!(key(&app_with("  \n")), Ok(None));
    }

    #[test]
    fn a_damaged_key_refuses_instead_of_skipping() {
        assert!(key(&app_with("abc")).is_err());
        let not_hex: &'static str = Box::leak("g".repeat(64).into_boxed_str());
        assert!(key(&app_with(not_hex)).is_err());
    }

    #[test]
    fn a_signed_list_gives_its_checksum_and_a_tampered_one_does_not() {
        let app = app_with(leaked_key());
        let key = key(&app).unwrap().unwrap();
        let hash = "a".repeat(64);
        let sums = format!("{hash}  psg-pdf-0.20.0-windows-x64.msi\n");
        let sig = pair().sign(sums.as_bytes());
        assert_eq!(signed_checksum(&app, &key, sums.as_bytes(), sig.as_ref(), "psg-pdf-0.20.0-windows-x64.msi"), Ok(hash.clone()));
        assert!(signed_checksum(&app, &key, sums.as_bytes(), sig.as_ref(), "other.msi").unwrap_err().contains("don't list"));
        let tampered = sums.replace('a', "b");
        assert!(
            signed_checksum(&app, &key, tampered.as_bytes(), sig.as_ref(), "psg-pdf-0.20.0-windows-x64.msi").unwrap_err().contains("aren't signed")
        );
        assert!(signed_checksum(&app, &key, sums.as_bytes(), &[0u8; 10], "psg-pdf-0.20.0-windows-x64.msi").is_err());
        let other = Ed25519KeyPair::from_seed_unchecked(&[8u8; 32]).unwrap().sign(sums.as_bytes());
        assert!(signed_checksum(&app, &key, sums.as_bytes(), other.as_ref(), "psg-pdf-0.20.0-windows-x64.msi").is_err());
    }

    /// What the release workflow makes: `openssl pkeyutl -sign -rawin` (packaging/ci/release-files.sh).
    #[test]
    fn a_signature_made_by_openssl_verifies() {
        let app = app_with("d463aff431c32f877bc63448431f908ef835a2ec2a7ff76526c1cef9aa9744c4");
        let key = key(&app).unwrap().unwrap();
        let sig_hex =
            "77b46111075e9f29d4593cdcfc3d468d2ef81eb4fd1405ba860eef17ce39fffd914f2a85798261ec61b66aa6652af91d89214dbf2ac07ea95a5146eed5850d08";
        let sig: Vec<u8> = (0..sig_hex.len()).step_by(2).map(|i| u8::from_str_radix(&sig_hex[i..i + 2], 16).unwrap()).collect();
        assert!(verify(&key, b"abc  x\n", &sig));
        assert!(!verify(&key, b"abd  x\n", &sig));
    }

    #[test]
    fn a_download_is_checked_again_against_the_list_kept_beside_it() {
        let app = app_with(leaked_key());
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("psg-pdf-0.20.0-windows-x64.msi");
        std::fs::write(&file, b"installer").unwrap();
        let hash = crate::http::sha256_file(&file).unwrap();
        let sums = format!("{hash}  psg-pdf-0.20.0-windows-x64.msi\n");
        let sig = pair().sign(sums.as_bytes());

        assert!(verify_download(&app, &file).unwrap_err().contains("missing"), "nothing kept beside it");
        keep_beside(&file, sums.as_bytes(), sig.as_ref()).unwrap();
        assert_eq!(verify_download(&app, &file), Ok(()));

        std::fs::write(&file, b"something else").unwrap();
        assert!(verify_download(&app, &file).unwrap_err().contains("changed after"));

        forget_beside(&file);
        assert!(verify_download(&app, &file).is_err());
        // Without a key there is nothing to check here.
        assert_eq!(verify_download(&App::new("a", "A", "", "o/a"), &file), Ok(()));
    }
}
