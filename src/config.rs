use std::{
    env, fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context;
use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::{
    auth::InternalAuth, cover_policy::CoverUrlPolicy, cover_proxy::CoverProxy, crypto::SecretBox,
};

#[derive(Clone)]
pub struct ServeConfig {
    pub bind: SocketAddr,
    pub database_url: String,
    pub production: bool,
    pub internal_auth: InternalAuth,
    pub secrets: SecretBox,
    pub cover_url_policy: CoverUrlPolicy,
    pub cover_proxy: CoverProxy,
    pub bootstrap_admin_email: String,
    pub bootstrap_admin_password: Option<String>,
    pub static_dir: PathBuf,
}

impl ServeConfig {
    pub fn from_runtime() -> anyhow::Result<Self> {
        let database_url = required("SUNSHINE_MANAGER_DATABASE_URL")?;
        if !database_url.starts_with("sqlite:") && !database_url.starts_with("sqlite://") {
            anyhow::bail!("SUNSHINE_MANAGER_DATABASE_URL must be a SQLite URL");
        }

        let credential_key = decode_key(&required("SUNSHINE_MANAGER_CREDENTIAL_KEY")?)?;
        let bind: SocketAddr = value("SUNSHINE_MANAGER_BIND", "127.0.0.1:18104")
            .parse()
            .context("SUNSHINE_MANAGER_BIND must be a socket address")?;
        let production = parse_bool("SUNSHINE_MANAGER_PRODUCTION", true)?;
        let static_dir =
            validate_static_dir(&required("SUNSHINE_MANAGER_STATIC_DIR")?, production)?;
        let session_absolute_ttl =
            Duration::from_secs(parse_u64("SUNSHINE_MANAGER_SESSION_TTL_SECONDS", 43_200)?);
        let session_idle_ttl = Duration::from_secs(parse_u64(
            "SUNSHINE_MANAGER_SESSION_IDLE_TTL_SECONDS",
            1_800,
        )?);
        let cookie_secure = parse_bool("SUNSHINE_MANAGER_SESSION_COOKIE_SECURE", production)?;
        if production && !cookie_secure {
            anyhow::bail!("production requires Secure session cookies");
        }
        if !cookie_secure && !bind.ip().is_loopback() {
            anyhow::bail!("insecure development cookies require a loopback bind address");
        }

        let cover_url_policy =
            CoverUrlPolicy::from_csv(&value("SUNSHINE_MANAGER_COVER_URL_ALLOWLIST", ""))?;
        let cover_proxy = match env::var("SUNSHINE_MANAGER_COVER_PROXY_ORIGIN") {
            Ok(origin) => CoverProxy::from_origin(&origin)?,
            Err(env::VarError::NotPresent) if cover_url_policy.is_empty() => CoverProxy::disabled(),
            Err(env::VarError::NotPresent) => anyhow::bail!(
                "SUNSHINE_MANAGER_COVER_PROXY_ORIGIN is required when cover uploads are enabled"
            ),
            Err(error) => return Err(error.into()),
        };

        Ok(Self {
            bind,
            database_url,
            production,
            internal_auth: InternalAuth::new(
                session_absolute_ttl,
                session_idle_ttl,
                cookie_secure,
            )?,
            secrets: SecretBox::new(
                value("SUNSHINE_MANAGER_CREDENTIAL_KEY_ID", "primary"),
                credential_key,
            )?,
            cover_url_policy,
            cover_proxy,
            bootstrap_admin_email: value(
                "SUNSHINE_MANAGER_BOOTSTRAP_ADMIN_EMAIL",
                "admin@example.com",
            ),
            bootstrap_admin_password: env::var("SUNSHINE_MANAGER_BOOTSTRAP_ADMIN_PASSWORD").ok(),
            static_dir,
        })
    }
}

fn validate_static_dir(value: &str, production: bool) -> anyhow::Result<PathBuf> {
    let configured = Path::new(value);
    anyhow::ensure!(
        configured.is_absolute(),
        "SUNSHINE_MANAGER_STATIC_DIR must be an absolute path"
    );
    let root = fs::canonicalize(configured)
        .context("SUNSHINE_MANAGER_STATIC_DIR must resolve to an existing directory")?;
    validate_static_tree(&root, production)?;
    anyhow::ensure!(
        root.join("index.html").is_file(),
        "SUNSHINE_MANAGER_STATIC_DIR must contain index.html"
    );
    anyhow::ensure!(
        root.join("assets").is_dir(),
        "SUNSHINE_MANAGER_STATIC_DIR must contain the current assets directory"
    );
    let mut root_entries = fs::read_dir(&root)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<Vec<_>, _>>()?;
    root_entries.sort();
    anyhow::ensure!(
        root_entries
            == [
                std::ffi::OsString::from("assets"),
                std::ffi::OsString::from("index.html"),
            ],
        "SUNSHINE_MANAGER_STATIC_DIR is not the exact current asset layout"
    );
    Ok(root)
}

fn validate_static_tree(root: &Path, production: bool) -> anyhow::Result<()> {
    let mut pending = vec![(root.to_path_buf(), 0_usize)];
    let mut entries = 0_usize;
    while let Some((path, depth)) = pending.pop() {
        anyhow::ensure!(depth <= 32, "static asset tree exceeds maximum depth");
        entries += 1;
        anyhow::ensure!(entries <= 10_000, "static asset tree is too large");
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("inspect static asset {}", path.display()))?;
        anyhow::ensure!(
            !metadata.file_type().is_symlink() && (metadata.is_dir() || metadata.is_file()),
            "static assets must contain only real directories and regular files"
        );
        #[cfg(unix)]
        validate_static_metadata(&metadata, production)?;
        if metadata.is_dir() {
            for child in fs::read_dir(&path)
                .with_context(|| format!("read static asset directory {}", path.display()))?
            {
                pending.push((child?.path(), depth + 1));
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn validate_static_metadata(metadata: &fs::Metadata, production: bool) -> anyhow::Result<()> {
    use std::os::unix::fs::MetadataExt;

    if production {
        anyhow::ensure!(
            metadata.uid() != rustix::process::geteuid().as_raw(),
            "production static assets must not be owned by the service account"
        );
        anyhow::ensure!(
            metadata.mode() & 0o022 == 0,
            "production static assets must not be group- or world-writable"
        );
    }
    if metadata.is_file() {
        anyhow::ensure!(
            metadata.nlink() == 1,
            "static asset files must have exactly one hard link"
        );
    }
    Ok(())
}

fn decode_key(value: &str) -> anyhow::Result<[u8; 32]> {
    let decoded = STANDARD
        .decode(value.trim())
        .context("SUNSHINE_MANAGER_CREDENTIAL_KEY must be base64")?;
    decoded.try_into().map_err(|_| {
        anyhow::anyhow!("SUNSHINE_MANAGER_CREDENTIAL_KEY must decode to exactly 32 bytes")
    })
}

fn required(name: &str) -> anyhow::Result<String> {
    env::var(name).with_context(|| format!("{name} is required"))
}

fn value(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_string())
}

fn parse_u64(name: &str, default: u64) -> anyhow::Result<u64> {
    value(name, &default.to_string())
        .parse()
        .with_context(|| format!("{name} must be an unsigned integer"))
}

fn parse_bool(name: &str, default: bool) -> anyhow::Result<bool> {
    value(name, if default { "true" } else { "false" })
        .parse()
        .with_context(|| format!("{name} must be true or false"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_key_decoding_requires_exactly_32_bytes() {
        assert!(decode_key(&STANDARD.encode([7_u8; 32])).is_ok());
        assert!(decode_key(&STANDARD.encode([7_u8; 31])).is_err());
    }

    #[test]
    fn static_directory_is_absolute_complete_and_contains_no_link_aliases() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("index.html"), "current").unwrap();
        fs::create_dir(directory.path().join("assets")).unwrap();
        fs::write(directory.path().join("assets/app.js"), "current").unwrap();

        assert_eq!(
            validate_static_dir(directory.path().to_str().unwrap(), false).unwrap(),
            directory.path().canonicalize().unwrap()
        );
        assert!(validate_static_dir(directory.path().to_str().unwrap(), true).is_err());
        assert!(validate_static_dir("web/dist", false).is_err());

        fs::remove_file(directory.path().join("index.html")).unwrap();
        assert!(validate_static_dir(directory.path().to_str().unwrap(), false).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn static_directory_rejects_symbolic_and_hard_linked_assets() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let index = directory.path().join("index.html");
        fs::write(&index, "current").unwrap();
        fs::create_dir(directory.path().join("assets")).unwrap();
        fs::write(directory.path().join("assets/app.js"), "current").unwrap();
        let alias = directory.path().join("alias.html");
        fs::hard_link(&index, &alias).unwrap();
        assert!(validate_static_dir(directory.path().to_str().unwrap(), false).is_err());

        fs::remove_file(&alias).unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        symlink(outside.path(), directory.path().join("linked.js")).unwrap();
        assert!(validate_static_dir(directory.path().to_str().unwrap(), false).is_err());
    }
}
