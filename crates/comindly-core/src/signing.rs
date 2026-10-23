//! Optional SSH signatures on ledger records, with the key developers already sign git commits
//! with (`ssh-keygen -Y`, namespace `comindly-ledger`). A signature that does not verify against
//! `allowed_signers` drops the record to T0; unsigned records are unaffected.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::ledger::Record;

pub const NAMESPACE: &str = "comindly-ledger";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    /// Public key, `ssh-ed25519 AAAA…`.
    pub key: String,
    /// Armored SSHSIG over the record without its signature.
    pub sig: String,
}

/// The bytes a signature covers: the record without its signature.
pub fn canonical(rec: &Record) -> Vec<u8> {
    let mut r = rec.clone();
    r.signature = None;
    serde_json::to_vec(&r).unwrap_or_default()
}

pub fn sign_with(rec: &mut Record, key: &Path) -> Result<()> {
    let public = public_key(key)?;
    let dir = scratch()?;
    let data = dir.join("record.json");
    std::fs::write(&data, canonical(rec))?;
    let ok = Command::new("ssh-keygen")
        .args(["-Y", "sign", "-n", NAMESPACE, "-f"])
        .arg(key)
        .arg(&data)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .context("run ssh-keygen")?;
    if !ok.status.success() {
        let _ = std::fs::remove_dir_all(&dir);
        bail!(
            "ssh-keygen could not sign: {}",
            String::from_utf8_lossy(&ok.stderr).trim()
        );
    }
    let sig = std::fs::read_to_string(dir.join("record.json.sig"))?;
    let _ = std::fs::remove_dir_all(&dir);
    rec.signature = Some(Signature { key: public, sig });
    Ok(())
}

/// Checks record signatures against an `allowed_signers` file, when there is one.
#[derive(Debug, Clone, Default)]
pub struct Verifier {
    allowed: Option<PathBuf>,
    cache: Option<PathBuf>,
}

impl Verifier {
    /// `cache`: a folder that remembers results, so each record is checked once.
    pub fn new(allowed: Option<PathBuf>, cache: Option<PathBuf>) -> Self {
        Verifier { allowed, cache }
    }

    /// `Some(true)` verified, `Some(false)` forged or tampered or by an unknown key, `None` when
    /// the record is unsigned or there is no `allowed_signers`. Results are cached by content.
    pub fn verify(&self, rec: &Record) -> Option<bool> {
        rec.signature.as_ref()?;
        verify_with(rec, self.allowed.as_deref()?, self.cache.as_deref())
    }
}

pub fn verify_with(rec: &Record, allowed: &Path, cache: Option<&Path>) -> Option<bool> {
    let s = rec.signature.as_ref()?;
    let allowed_text = std::fs::read(allowed).ok()?;
    let mut h = blake3::Hasher::new();
    h.update(&canonical(rec));
    h.update(s.sig.as_bytes());
    h.update(&allowed_text);
    let cache = cache.map(|d| d.join(h.finalize().to_hex().as_str()));
    if let Some(v) = cache.as_ref().and_then(|c| std::fs::read_to_string(c).ok()) {
        return Some(v == "1");
    }
    let ok = check(rec, s, allowed).unwrap_or(false);
    if let Some(c) = cache {
        let _ = std::fs::create_dir_all(c.parent().unwrap_or(Path::new(".")));
        let _ = std::fs::write(c, if ok { "1" } else { "0" });
    }
    Some(ok)
}

fn check(rec: &Record, s: &Signature, allowed: &Path) -> Result<bool> {
    let dir = scratch()?;
    let sig = dir.join("record.sig");
    std::fs::write(&sig, &s.sig)?;
    // Which allowed principal holds the signing key (used transiently, never stored).
    let found = Command::new("ssh-keygen")
        .args(["-Y", "find-principals", "-s"])
        .arg(&sig)
        .arg("-f")
        .arg(allowed)
        .output()?;
    let principal = String::from_utf8_lossy(&found.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .to_string();
    if !found.status.success() || principal.is_empty() {
        let _ = std::fs::remove_dir_all(&dir);
        return Ok(false);
    }
    let mut child = Command::new("ssh-keygen")
        .args(["-Y", "verify", "-n", NAMESPACE, "-I", &principal, "-f"])
        .arg(allowed)
        .arg("-s")
        .arg(&sig)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    {
        use std::io::Write;
        let mut stdin = child.stdin.take().context("stdin")?;
        stdin.write_all(&canonical(rec))?;
    }
    let ok = child.wait()?.success();
    let _ = std::fs::remove_dir_all(&dir);
    Ok(ok)
}

fn public_key(key: &Path) -> Result<String> {
    let pubfile = if key.extension().is_some_and(|e| e == "pub") {
        key.to_path_buf()
    } else {
        PathBuf::from(format!("{}.pub", key.display()))
    };
    let text = std::fs::read_to_string(&pubfile)
        .with_context(|| format!("read public key {}", pubfile.display()))?;
    Ok(text
        .split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" "))
}

fn scratch() -> Result<PathBuf> {
    let d = std::env::temp_dir().join(format!("comindly-sign-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::{AuthorKind, Kind, Provenance};

    #[test]
    fn sign_verify_and_reject_tampering() {
        if Command::new("ssh-keygen").arg("-?").output().is_err() {
            return; // no OpenSSH on this machine
        }
        let dir = scratch().unwrap();
        let key = dir.join("k");
        let made = Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-C", "t", "-f"])
            .arg(&key)
            .status()
            .unwrap();
        assert!(made.success());
        let public = public_key(&key).unwrap();
        let allowed = dir.join("allowed");
        std::fs::write(&allowed, format!("dev@example {public}\n")).unwrap();
        let mut rec = Record::new(
            Kind::Gotcha,
            "ids are customer-scoped",
            Vec::new(),
            Provenance {
                author: AuthorKind::Human,
                actor: "a".into(),
                source: "t".into(),
                agent: None,
                session: None,
                name: None,
            },
        );
        sign_with(&mut rec, &key).unwrap();
        assert_eq!(rec.signature.as_ref().unwrap().key, public);
        assert_eq!(verify_with(&rec, &allowed, None), Some(true));
        let mut forged = rec.clone();
        forged.title = "ids are global".into();
        assert_eq!(verify_with(&forged, &allowed, None), Some(false));
        std::fs::write(&allowed, "").unwrap();
        assert_eq!(
            verify_with(&rec, &allowed, None),
            Some(false),
            "unknown key"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
