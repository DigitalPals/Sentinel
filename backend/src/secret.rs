//! Small encryption layer for credentials stored in PostgreSQL.
//!
//! Values are encrypted before they are written and transparently decrypted
//! after they are read. Existing plaintext values are still accepted so older
//! installations migrate in place on the next startup.

use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{anyhow, Context};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use sha2::{Digest, Sha256};

use crate::notify::NotificationSettings;

const PREFIX: &str = "enc:v1:";
const FALLBACK_SALT: &[u8] = b"cybex-sentinel-local-secret-v1";

pub fn is_sealed(value: &str) -> bool {
    value.starts_with(PREFIX)
}

pub fn seal(value: &str) -> anyhow::Result<String> {
    if value.is_empty() || is_sealed(value) {
        return Ok(value.to_string());
    }
    let cipher = cipher();
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let encrypted = cipher
        .encrypt(Nonce::from_slice(&nonce), value.as_bytes())
        .map_err(|_| anyhow!("encrypting secret"))?;
    Ok(format!(
        "{PREFIX}{}:{}",
        URL_SAFE_NO_PAD.encode(nonce),
        URL_SAFE_NO_PAD.encode(encrypted)
    ))
}

pub fn open(value: &str) -> anyhow::Result<String> {
    if value.is_empty() || !is_sealed(value) {
        return Ok(value.to_string());
    }
    let raw = value.trim_start_matches(PREFIX);
    let (nonce, encrypted) = raw
        .split_once(':')
        .ok_or_else(|| anyhow!("invalid encrypted secret envelope"))?;
    let nonce = URL_SAFE_NO_PAD
        .decode(nonce)
        .context("decoding encrypted secret nonce")?;
    if nonce.len() != 12 {
        return Err(anyhow!("invalid encrypted secret nonce length"));
    }
    let encrypted = URL_SAFE_NO_PAD
        .decode(encrypted)
        .context("decoding encrypted secret payload")?;
    for key in decryption_keys() {
        if let Ok(decrypted) =
            cipher_for(&key).decrypt(Nonce::from_slice(&nonce), encrypted.as_ref())
        {
            return String::from_utf8(decrypted).context("secret was not valid UTF-8");
        }
    }
    Err(anyhow!("decrypting secret"))
}

pub fn open_notifications(settings: &mut NotificationSettings) -> anyhow::Result<()> {
    settings.email.smtp_password = open(&settings.email.smtp_password)?;
    settings.slack.webhook_url = open(&settings.slack.webhook_url)?;
    settings.telegram.bot_token = open(&settings.telegram.bot_token)?;
    settings.push.vapid_private_key = open(&settings.push.vapid_private_key)?;
    Ok(())
}

pub fn seal_notifications(settings: &mut NotificationSettings) -> anyhow::Result<()> {
    settings.email.smtp_password = seal(&settings.email.smtp_password)?;
    settings.slack.webhook_url = seal(&settings.slack.webhook_url)?;
    settings.telegram.bot_token = seal(&settings.telegram.bot_token)?;
    settings.push.vapid_private_key = seal(&settings.push.vapid_private_key)?;
    Ok(())
}

pub fn uses_fallback_key() -> bool {
    std::env::var("SENTINEL_SECRET_KEY")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .is_none()
}

fn cipher() -> Aes256Gcm {
    cipher_for(&primary_key_bytes())
}

fn cipher_for(key: &[u8; 32]) -> Aes256Gcm {
    Aes256Gcm::new_from_slice(key).expect("AES-256 key length is fixed")
}

fn primary_key_bytes() -> [u8; 32] {
    if let Ok(raw) = std::env::var("SENTINEL_SECRET_KEY") {
        let raw = raw.trim();
        if !raw.is_empty() {
            return key_from_config_value(raw);
        }
    }

    Sha256::digest(FALLBACK_SALT).into()
}

fn key_from_config_value(raw: &str) -> [u8; 32] {
    if let Ok(decoded) = URL_SAFE_NO_PAD.decode(raw) {
        if decoded.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&decoded);
            return key;
        }
    }
    Sha256::digest(raw.as_bytes()).into()
}

fn legacy_fallback_key(db: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(FALLBACK_SALT);
    h.update(db.as_bytes());
    h.finalize().into()
}

fn decryption_keys() -> Vec<[u8; 32]> {
    let mut keys = Vec::new();
    push_unique(&mut keys, primary_key_bytes());

    if let Ok(db) = std::env::var("DATABASE_URL") {
        push_unique(&mut keys, legacy_fallback_key(&db));
        for variant in database_url_variants(&db) {
            push_unique(&mut keys, legacy_fallback_key(&variant));
        }
    }

    keys
}

fn database_url_variants(db: &str) -> Vec<String> {
    let mut variants = Vec::new();
    if db.contains("@127.0.0.1:5432/") {
        variants.push(db.replace("@127.0.0.1:5432/", "@db:5432/"));
    }
    if db.contains("@localhost:5432/") {
        variants.push(db.replace("@localhost:5432/", "@db:5432/"));
    }
    if db.contains("@db:5432/") {
        variants.push(db.replace("@db:5432/", "@127.0.0.1:5432/"));
    }
    variants
}

fn push_unique(keys: &mut Vec<[u8; 32]>, key: [u8; 32]) {
    if !keys.iter().any(|existing| existing == &key) {
        keys.push(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_round_trip() {
        let sealed = seal("super-secret").unwrap();
        assert!(is_sealed(&sealed));
        assert_eq!(open(&sealed).unwrap(), "super-secret");
    }

    #[test]
    fn plaintext_is_backwards_compatible() {
        assert_eq!(open("old-value").unwrap(), "old-value");
        assert_eq!(seal("").unwrap(), "");
    }
}
