use std::path::PathBuf;

use mongodb::{
    options::{ClientOptions, DriverInfo, Tls},
    Client,
};

use crate::interface_types::MongoAgentError;

const DRIVER_NAME: &str = "Hasura";

const DEFAULT_CA_BUNDLE_PATHS: &[&str] = &[
    "/etc/ssl/certs/ca-bundle.crt",
    "/etc/ssl/certs/ca-certificates.crt",
];

pub async fn get_mongodb_client(database_uri: &str) -> Result<Client, MongoAgentError> {
    // An extra line of code to work around a DNS issue on Windows:
    let mut options = ClientOptions::parse(database_uri).await?;

    // Helps MongoDB to collect statistics on Hasura use
    options.driver_info = Some(DriverInfo::builder().name(DRIVER_NAME).build());

    // The `mongodb` crate uses compiled-in `webpki-roots` rather than the OS truststore, so it
    // ignores `SSL_CERT_FILE` and system CA bundles (e.g. Amazon RDS / DocumentDB roots) unless
    // `ca_file_path` is explicitly provided. Default to `SSL_CERT_FILE` or the container's CA
    // bundle whenever TLS is enabled and the caller did not specify `tlsCAFile`.
    apply_default_ca_file(&mut options, resolve_default_ca_file());

    let client = Client::with_options(options)?;
    Ok(client)
}

fn resolve_default_ca_file() -> Option<PathBuf> {
    if let Ok(env_path) = std::env::var("SSL_CERT_FILE") {
        if !env_path.is_empty() {
            let path = PathBuf::from(env_path);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    DEFAULT_CA_BUNDLE_PATHS
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
}

fn apply_default_ca_file(options: &mut ClientOptions, default_ca_file: Option<PathBuf>) {
    if let (Some(Tls::Enabled(tls_opts)), Some(ca_path)) = (&mut options.tls, default_ca_file) {
        if tls_opts.ca_file_path.is_none() {
            tls_opts.ca_file_path = Some(ca_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mongodb::options::TlsOptions;

    #[tokio::test]
    async fn applies_default_ca_file_when_tls_enabled_and_unset() {
        let mut opts = ClientOptions::parse("mongodb://localhost:27017/db?tls=true")
            .await
            .unwrap();
        let expected = PathBuf::from("/etc/ssl/certs/ca-bundle.crt");
        apply_default_ca_file(&mut opts, Some(expected.clone()));

        match opts.tls {
            Some(Tls::Enabled(tls)) => assert_eq!(tls.ca_file_path, Some(expected)),
            other => panic!("expected Tls::Enabled, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn preserves_explicit_tls_ca_file() {
        let mut opts = ClientOptions::parse("mongodb://localhost:27017/db?tls=true")
            .await
            .unwrap();
        let explicit = PathBuf::from("/custom/ca.pem");
        opts.tls = Some(Tls::Enabled(
            TlsOptions::builder()
                .ca_file_path(explicit.clone())
                .build(),
        ));
        apply_default_ca_file(&mut opts, Some(PathBuf::from("/etc/ssl/certs/ca-bundle.crt")));

        match opts.tls {
            Some(Tls::Enabled(tls)) => assert_eq!(tls.ca_file_path, Some(explicit)),
            other => panic!("expected Tls::Enabled, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn leaves_plaintext_connection_untouched() {
        let mut opts = ClientOptions::parse("mongodb://localhost:27017/db")
            .await
            .unwrap();
        apply_default_ca_file(&mut opts, Some(PathBuf::from("/etc/ssl/certs/ca-bundle.crt")));
        assert!(opts.tls.is_none());
    }
}
