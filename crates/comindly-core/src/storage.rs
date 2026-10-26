//! Index storage at any location LanceDB opens: a folder, `s3://`, `gs://` or `az://`.
//! Credentials, region and endpoint come from the platform's own environment, never from comindly.

use std::sync::Arc;

use anyhow::{Context, Result};
use object_store::path::Path as ObjPath;
use object_store::ObjectStore;

/// A LanceDB connection builder for `uri`, on the process's one session.
pub fn connect(uri: &str) -> lancedb::connection::ConnectBuilder {
    lancedb::connect(uri).session(session())
}

/// Caches and object stores shared by every connection, bounded instead of Lance's 7 GB default.
fn session() -> Arc<lancedb::Session> {
    static SESSION: std::sync::OnceLock<Arc<lancedb::Session>> = std::sync::OnceLock::new();
    SESSION
        .get_or_init(|| {
            Arc::new(lancedb::Session::new(
                256 << 20,
                64 << 20,
                Arc::new(lancedb::ObjectStoreRegistry::default()),
            ))
        })
        .clone()
}

/// The object store and path for a remote `uri`, with the same credentials LanceDB uses.
pub fn open(uri: &str) -> Result<(Arc<dyn ObjectStore>, ObjPath)> {
    let (store, path) = crate::rt::block_on(lance_io::object_store::ObjectStore::from_uri(uri))
        .with_context(|| format!("open {uri}"))?;
    Ok((store.inner.clone(), path))
}

/// `text` with any `user:password@` removed from URLs in it.
pub fn without_userinfo(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("://") {
        let (head, tail) = rest.split_at(i + 3);
        out.push_str(head);
        let end = tail
            .find(|c: char| c == '/' || c.is_whitespace())
            .unwrap_or(tail.len());
        match tail[..end].rfind('@') {
            Some(at) => rest = &tail[at + 1..],
            None => rest = tail,
        }
    }
    out.push_str(rest);
    out
}

/// A short, secret-free explanation of an error, with storage error replies turned into what to do.
pub fn explain(raw: &str) -> String {
    if raw.contains("does not support timezone offsets") {
        return "The AWS SSO token cache has an expiry time the AWS SDK cannot read; sign in again \
                (`aws sso login`)"
            .into();
    }
    if raw.contains("Failed to get AWS credentials")
        || raw.contains("CredentialsNotLoaded")
        || raw.contains("no credentials")
    {
        return "No storage credentials: run comindly where the platform's usual credentials are \
                available (an IAM or workload role, AWS_PROFILE, GOOGLE_APPLICATION_CREDENTIALS, \
                AZURE_STORAGE_ACCOUNT_NAME …)"
            .into();
    }
    let code = raw
        .split("<Code>")
        .nth(1)
        .and_then(|r| r.split("</Code>").next())
        .map(str::to_string);
    let hint = match code.as_deref() {
        Some("ExpiredToken" | "TokenRefreshRequired") => {
            Some("the storage credentials have expired; renew them and try again")
        }
        Some("AccessDenied" | "AuthorizationFailure" | "AuthorizationPermissionMismatch") => Some(
            "the storage service denied access; check this machine's permissions on the bucket",
        ),
        Some("NoSuchBucket" | "ContainerNotFound") => {
            Some("the bucket does not exist (check the name and region)")
        }
        Some(
            "InvalidAccessKeyId"
            | "SignatureDoesNotMatch"
            | "InvalidToken"
            | "AuthenticationFailed",
        ) => Some("the storage service rejected the credentials"),
        _ => None,
    };
    match (hint, code) {
        (Some(h), Some(c)) => format!("{h} ({c})"),
        _ => redact(raw),
    }
}

/// Removes session tokens and other long credential-like strings.
fn redact(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut word = String::new();
    let flush = |w: &mut String, out: &mut String| {
        let has = |f: fn(&char) -> bool| w.chars().any(|c| f(&c));
        let path = w.starts_with('/') || w.starts_with('~');
        let secretish = !path
            && w.len() >= 60
            && has(char::is_ascii_uppercase)
            && has(char::is_ascii_lowercase)
            && has(char::is_ascii_digit);
        out.push_str(if secretish { "[redacted]" } else { w.as_str() });
        w.clear();
    };
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || "+/=_-%".contains(c) {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

/// An index in object storage (`s3://`, `gs://`, `az://`), not a folder on this machine.
pub fn is_remote(index_root: &str) -> bool {
    index_root.contains("://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_credentials_from_urls() {
        assert_eq!(
            without_userinfo("fatal: https://oauth2:abc123@gitlab.example.com/a/b.git not found"),
            "fatal: https://gitlab.example.com/a/b.git not found"
        );
        assert_eq!(
            without_userinfo("git@github.com:a/b.git"),
            "git@github.com:a/b.git"
        );
        assert_eq!(
            without_userinfo("https://github.com/a/b"),
            "https://github.com/a/b"
        );
    }

    #[test]
    fn explains_s3_replies_without_secrets() {
        let token = &"ExampleSessionToken0123456789".repeat(3);
        let raw = format!(
            "ListRequest: <Error><Code>ExpiredToken</Code><Token-0>{token}</Token-0></Error>"
        );
        let e = explain(&raw);
        assert!(e.contains("(ExpiredToken)") && !e.contains(token), "{e}");
        let other = explain(&format!("connect failed: token={token} at s3://bucket/x"));
        assert!(
            !other.contains(token) && other.contains("s3://bucket/x"),
            "{other}"
        );
        let path = format!("/home/{}", "long-folder-name-".repeat(6));
        assert_eq!(explain(&path), path);
        let offset = "Failed to get AWS credentials: InvalidField { field: \"expiresAt\", source: DateTimeParseError { kind: Invalid(\"Smithy does not support timezone offsets in RFC-3339 date times\") } }";
        assert!(explain(offset).contains("aws sso login"));
        let chain = "Failed to get AWS credentials: CredentialsNotLoaded(CredentialsNotLoaded { source: Some(ProviderChainError { attempts: [] }) })";
        assert!(
            explain(chain).starts_with("No storage credentials"),
            "{}",
            explain(chain)
        );
        let mixed = "/Users/Someone/Library/Caches/tmp-Ab12Cd34Ef56/index/_graph/symbols.lance";
        assert_eq!(explain(mixed), mixed);
    }
}
