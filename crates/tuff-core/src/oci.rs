//! OCI registry distribution for deterministic Tuff pack artifacts.

use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

use docker_credential::{CredentialRetrievalError, DockerCredential};
use oci_client::{
    Client, Reference, RegistryOperation,
    client::{Certificate, CertificateEncoding, ClientConfig, ClientProtocol},
    errors::{OciDistributionError, OciErrorCode},
    manifest::{OCI_IMAGE_MEDIA_TYPE, OciDescriptor, OciImageManifest},
    secrets::RegistryAuth,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    error::{Result, TuffError},
    pack,
};

/// OCI artifact type identifying a Tuff pack manifest.
pub const PACK_ARTIFACT_MEDIA_TYPE: &str = "application/vnd.tuff.pack.v1";
/// OCI layer media type containing exact `.tuffpack` bytes.
pub const PACK_LAYER_MEDIA_TYPE: &str = "application/vnd.tuff.pack.layer.v1";
/// OCI media type for the standard empty JSON descriptor.
pub const OCI_EMPTY_MEDIA_TYPE: &str = "application/vnd.oci.empty.v1+json";

const OCI_EMPTY_JSON: &[u8] = b"{}";
const OCI_EMPTY_DIGEST: &str =
    "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a";
const OCI_TITLE_ANNOTATION: &str = "org.opencontainers.image.title";
const OCI_VERSION_ANNOTATION: &str = "org.opencontainers.image.version";
const OCI_DESCRIPTION_ANNOTATION: &str = "org.opencontainers.image.description";

/// Network and TLS settings shared by OCI push and pull operations.
#[derive(Debug, Clone, Default)]
pub struct OciTransferOptions {
    /// Use unencrypted HTTP instead of HTTPS for a development registry.
    pub plain_http: bool,
    /// Additional PEM-encoded certificate authorities trusted for this operation.
    pub ca_files: Vec<PathBuf>,
}

/// Deterministic result returned after publishing a pack.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OciPushResult {
    pub status: OciPushStatus,
    pub name: String,
    pub version: String,
    pub artifact_digest: String,
    pub manifest_digest: String,
    pub tag_reference: String,
    pub reference: String,
}

/// Whether a push wrote a manifest or found the same manifest already published.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OciPushStatus {
    Pushed,
    Unchanged,
}

/// Deterministic result returned after pulling and verifying a pack.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OciPullResult {
    pub name: String,
    pub version: String,
    pub artifact_digest: String,
    pub manifest_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_reference: Option<String>,
    pub reference: String,
    pub output: String,
}

/// Publishes one verified `.tuffpack` artifact under an explicit OCI tag.
///
/// The existing tag is treated as immutable unless `force` is true. Publishing the exact same
/// manifest is idempotent and returns [`OciPushStatus::Unchanged`].
///
/// # Errors
///
/// Returns an error for an invalid artifact or reference, credential and TLS failures, a
/// conflicting tag, registry protocol failures, or a digest mismatch after publication.
pub async fn push_pack(
    artifact_path: &Path,
    reference: &str,
    force: bool,
    options: &OciTransferOptions,
) -> Result<OciPushResult> {
    let reference = parse_push_reference(reference)?;
    let artifact_bytes = fs::read(artifact_path).map_err(|error| {
        TuffError::new(format!(
            "could not read pack artifact {}: {error}",
            artifact_path.display()
        ))
    })?;
    let artifact = pack::read_artifact_bytes(&artifact_bytes)?;
    let artifact_digest = format!("sha256:{}", artifact.digest);
    let manifest = pack_manifest(&artifact, &artifact_digest, artifact_bytes.len())?;
    let manifest_bytes = serde_json::to_vec(&manifest)?;
    let expected_manifest_digest = sha256_digest(&manifest_bytes);
    let tag_reference = reference.whole();
    let digest_reference = digest_reference(&reference, &expected_manifest_digest);

    let client = registry_client(options)?;
    let auth = registry_auth(reference.registry())?;
    let existing = match client.fetch_manifest_digest(&reference, &auth).await {
        Ok(digest) => Some(digest),
        Err(error) if manifest_is_missing(&error) => None,
        Err(error) => return Err(oci_error("check existing OCI tag", error)),
    };
    if existing.as_deref() == Some(expected_manifest_digest.as_str()) {
        return Ok(OciPushResult {
            status: OciPushStatus::Unchanged,
            name: artifact.metadata.name,
            version: artifact.metadata.version,
            artifact_digest,
            manifest_digest: expected_manifest_digest,
            tag_reference,
            reference: digest_reference,
        });
    }
    if let Some(existing) = existing
        && !force
    {
        return Err(TuffError::new(format!(
            "refusing to move existing OCI tag '{tag_reference}' from {existing} to {expected_manifest_digest}; pass --force to replace it or publish a new tag"
        )));
    }

    client
        .auth(&reference, &auth, RegistryOperation::Push)
        .await
        .map_err(|error| oci_error("authenticate OCI push", error))?;
    push_blob_if_missing(&client, &reference, OCI_EMPTY_JSON, OCI_EMPTY_DIGEST).await?;
    push_blob_if_missing(&client, &reference, &artifact_bytes, &artifact_digest).await?;
    client
        .push_manifest_raw(
            &reference,
            manifest_bytes,
            OCI_IMAGE_MEDIA_TYPE.parse().map_err(|error| {
                TuffError::new(format!("invalid OCI manifest media type: {error}"))
            })?,
        )
        .await
        .map_err(|error| oci_error("publish OCI pack manifest", error))?;
    let published_digest = client
        .fetch_manifest_digest(&reference, &auth)
        .await
        .map_err(|error| oci_error("read back published OCI manifest", error))?;
    if published_digest != expected_manifest_digest {
        return Err(TuffError::new(format!(
            "published OCI manifest digest mismatch: expected {expected_manifest_digest}, registry returned {published_digest}"
        )));
    }

    Ok(OciPushResult {
        status: OciPushStatus::Pushed,
        name: artifact.metadata.name,
        version: artifact.metadata.version,
        artifact_digest,
        manifest_digest: expected_manifest_digest,
        tag_reference,
        reference: digest_reference,
    })
}

/// Normalize an arbitrary OCI reference into its repository form
/// ("registry/repository", no tag), so it can be recorded once and re-queried
/// for available tags later without a pinned tag going stale.
pub fn normalize_pack_repository(raw: &str) -> Result<String> {
    let reference = parse_reference(raw)?;
    Ok(format!(
        "{}/{}",
        reference.registry(),
        reference.repository()
    ))
}

/// List the tags published under a pack's repository.
///
/// `repository_reference` is the "registry/repository" form produced by
/// [`normalize_pack_repository`]; any tag on it is ignored, since listing
/// tags does not require pinning one.
pub async fn list_pack_versions(
    repository_reference: &str,
    options: &OciTransferOptions,
) -> Result<Vec<String>> {
    let reference = parse_reference(repository_reference)?;
    let client = registry_client(options)?;
    let auth = registry_auth(reference.registry())?;
    let response = client
        .list_tags(&reference, &auth, None, None)
        .await
        .map_err(|error| oci_error("list OCI pack tags", error))?;
    Ok(response.tags)
}

/// What a pack tag points at right now, learned from the manifest alone.
///
/// Resolving a tag costs one manifest fetch (a few hundred bytes of JSON)
/// and no blob download: a Tuff pack manifest carries exactly one layer,
/// and that layer's digest *is* the `.tuffpack` artifact digest the lockfile
/// records. Comparing the two answers "is the tag still the bytes I
/// installed?" without pulling the pack again.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OciResolvedTag {
    pub manifest_digest: String,
    /// The pack layer digest, `sha256:<hex>`, equal to the artifact digest.
    pub artifact_digest: String,
    /// Pack name from the manifest annotations, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Pack version from the manifest annotations, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Resolves a tag (or digest) reference to the pack it currently names.
///
/// Returns `Ok(None)` when the registry reports the tag as missing, which is
/// a distinct answer from a network or authentication failure: a deleted tag
/// is something the caller should say out loud, not fold into "error".
///
/// # Errors
///
/// Returns an error for an invalid reference, credential and TLS failures,
/// unsupported OCI metadata, or registry protocol failures other than a
/// missing manifest.
pub async fn resolve_pack_tag(
    reference: &str,
    options: &OciTransferOptions,
) -> Result<Option<OciResolvedTag>> {
    let requested = parse_pull_reference(reference)?;
    let client = registry_client(options)?;
    let auth = registry_auth(requested.registry())?;
    let manifest_digest = match client.fetch_manifest_digest(&requested, &auth).await {
        Ok(digest) => digest,
        Err(error) if manifest_is_missing(&error) => return Ok(None),
        Err(error) => return Err(oci_error("resolve OCI pack reference", error)),
    };
    let (_, manifest) = fetch_pack_manifest(&client, &auth, &requested, &manifest_digest).await?;
    let annotations = manifest.annotations.as_ref();
    Ok(Some(OciResolvedTag {
        manifest_digest,
        artifact_digest: manifest.layers[0].digest.clone(),
        name: annotations.and_then(|a| a.get(OCI_TITLE_ANNOTATION).cloned()),
        version: annotations.and_then(|a| a.get(OCI_VERSION_ANNOTATION).cloned()),
    }))
}

/// Pulls the manifest behind an already-resolved digest and validates its
/// shape as a Tuff pack manifest. Shared by [`resolve_pack_tag`] and
/// [`pull_pack`], which differ only in whether the layer is downloaded.
async fn fetch_pack_manifest(
    client: &Client,
    auth: &RegistryAuth,
    requested: &Reference,
    manifest_digest: &str,
) -> Result<(Reference, OciImageManifest)> {
    let pinned = Reference::with_digest(
        requested.registry().to_string(),
        requested.repository().to_string(),
        manifest_digest.to_string(),
    );
    let (manifest_bytes, pulled_digest) = client
        .pull_manifest_raw(&pinned, auth, &[OCI_IMAGE_MEDIA_TYPE])
        .await
        .map_err(|error| oci_error("pull OCI pack manifest", error))?;
    if pulled_digest != manifest_digest {
        return Err(TuffError::new(format!(
            "pulled OCI manifest digest mismatch: resolved {manifest_digest}, received {pulled_digest}"
        )));
    }
    let manifest: OciImageManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| TuffError::new(format!("invalid OCI pack manifest JSON: {error}")))?;
    validate_pack_manifest(&manifest)?;
    Ok((pinned, manifest))
}

/// Pulls one OCI-distributed pack, verifies both OCI and Tuff integrity, and persists it atomically.
///
/// # Errors
///
/// Returns an error for an invalid reference, existing output, credential and TLS failures,
/// unsupported OCI metadata, registry protocol failures, digest mismatches, or invalid pack bytes.
pub async fn pull_pack(
    reference: &str,
    output: &Path,
    options: &OciTransferOptions,
) -> Result<OciPullResult> {
    if output.exists() {
        return Err(TuffError::new(format!(
            "refusing to overwrite existing pack artifact: {}",
            output.display()
        )));
    }
    let requested = parse_pull_reference(reference)?;
    let tag_reference = requested.tag().map(|_| requested.whole());
    let client = registry_client(options)?;
    let auth = registry_auth(requested.registry())?;
    let manifest_digest = client
        .fetch_manifest_digest(&requested, &auth)
        .await
        .map_err(|error| oci_error("resolve OCI pack reference", error))?;
    let (pinned, manifest) =
        fetch_pack_manifest(&client, &auth, &requested, &manifest_digest).await?;
    let layer = &manifest.layers[0];

    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = tempfile::Builder::new()
        .prefix("tuff-oci-pull-")
        .tempfile_in(parent)?;
    let writer = tokio::fs::File::from_std(temporary.reopen()?);
    client
        .pull_blob(&pinned, layer, writer)
        .await
        .map_err(|error| oci_error("pull OCI pack layer", error))?;
    let downloaded_size = temporary.as_file().metadata()?.len();
    if downloaded_size != layer.size as u64 {
        return Err(TuffError::new(format!(
            "pulled OCI pack layer size mismatch: expected {}, received {downloaded_size}",
            layer.size
        )));
    }
    let artifact_bytes = fs::read(temporary.path())?;
    let artifact = pack::read_artifact_bytes(&artifact_bytes)?;
    let artifact_digest = format!("sha256:{}", artifact.digest);
    if layer.digest != artifact_digest {
        return Err(TuffError::new(format!(
            "OCI layer digest {} does not match Tuff artifact digest {artifact_digest}",
            layer.digest
        )));
    }
    validate_pack_annotations(&manifest, &artifact)?;
    temporary.persist_noclobber(output).map_err(|error| {
        TuffError::new(format!(
            "could not persist pulled pack artifact {}: {}",
            output.display(),
            error.error
        ))
    })?;
    let reference = digest_reference(&pinned, &manifest_digest);

    Ok(OciPullResult {
        name: artifact.metadata.name,
        version: artifact.metadata.version,
        artifact_digest,
        manifest_digest,
        tag_reference,
        reference,
        output: output.display().to_string(),
    })
}

fn parse_push_reference(raw: &str) -> Result<Reference> {
    if raw.contains('@') || !has_explicit_tag(raw) {
        return Err(TuffError::new(
            "OCI push reference must contain an explicit tag, for example ghcr.io/acme/engineering:1.2.0",
        ));
    }
    parse_reference(raw)
}

fn parse_pull_reference(raw: &str) -> Result<Reference> {
    if !raw.contains('@') && !has_explicit_tag(raw) {
        return Err(TuffError::new(
            "OCI pull reference must contain an explicit tag or digest; implicit 'latest' is not allowed",
        ));
    }
    parse_reference(raw)
}

fn parse_reference(raw: &str) -> Result<Reference> {
    if raw.trim() != raw || raw.contains("://") {
        return Err(TuffError::new(format!(
            "invalid OCI reference '{raw}'; use registry/repository:tag or registry/repository@sha256:digest without a URL scheme"
        )));
    }
    raw.parse::<Reference>()
        .map_err(|error| TuffError::new(format!("invalid OCI reference '{raw}': {error}")))
}

fn has_explicit_tag(raw: &str) -> bool {
    let name = raw.split('@').next().unwrap_or(raw);
    let slash = name.rfind('/');
    name.rfind(':')
        .is_some_and(|colon| slash.is_none_or(|slash| colon > slash))
}

fn pack_manifest(
    artifact: &pack::PackArtifact,
    artifact_digest: &str,
    artifact_size: usize,
) -> Result<OciImageManifest> {
    let artifact_size = i64::try_from(artifact_size)
        .map_err(|_| TuffError::new("pack artifact is too large for an OCI descriptor"))?;
    let mut annotations = BTreeMap::new();
    annotations.insert(
        OCI_TITLE_ANNOTATION.to_string(),
        artifact.metadata.name.clone(),
    );
    annotations.insert(
        OCI_VERSION_ANNOTATION.to_string(),
        artifact.metadata.version.clone(),
    );
    annotations.insert(
        OCI_DESCRIPTION_ANNOTATION.to_string(),
        artifact.metadata.description.clone(),
    );
    Ok(OciImageManifest {
        schema_version: 2,
        media_type: Some(OCI_IMAGE_MEDIA_TYPE.to_string()),
        config: descriptor(OCI_EMPTY_MEDIA_TYPE, OCI_EMPTY_DIGEST, 2),
        layers: vec![descriptor(
            PACK_LAYER_MEDIA_TYPE,
            artifact_digest,
            artifact_size,
        )],
        subject: None,
        artifact_type: Some(PACK_ARTIFACT_MEDIA_TYPE.to_string()),
        annotations: Some(annotations),
    })
}

fn descriptor(media_type: &str, digest: &str, size: i64) -> OciDescriptor {
    OciDescriptor {
        media_type: media_type.to_string(),
        digest: digest.to_string(),
        size,
        urls: None,
        annotations: None,
        artifact_type: None,
    }
}

fn validate_pack_manifest(manifest: &OciImageManifest) -> Result<()> {
    if manifest.schema_version != 2 || manifest.media_type.as_deref() != Some(OCI_IMAGE_MEDIA_TYPE)
    {
        return Err(TuffError::new(
            "OCI object is not an OCI image manifest schema version 2",
        ));
    }
    if manifest.artifact_type.as_deref() != Some(PACK_ARTIFACT_MEDIA_TYPE) {
        return Err(TuffError::new(format!(
            "OCI object is not a Tuff pack: expected artifact type {PACK_ARTIFACT_MEDIA_TYPE}"
        )));
    }
    if manifest.subject.is_some() {
        return Err(TuffError::new(
            "OCI Tuff pack manifest must not declare a subject",
        ));
    }
    if manifest.config.media_type != OCI_EMPTY_MEDIA_TYPE
        || manifest.config.digest != OCI_EMPTY_DIGEST
        || manifest.config.size != 2
    {
        return Err(TuffError::new(
            "OCI Tuff pack manifest has an invalid empty configuration descriptor",
        ));
    }
    if manifest.layers.len() != 1 {
        return Err(TuffError::new(format!(
            "OCI Tuff pack manifest must contain exactly one layer, found {}",
            manifest.layers.len()
        )));
    }
    let layer = &manifest.layers[0];
    if layer.media_type != PACK_LAYER_MEDIA_TYPE {
        return Err(TuffError::new(format!(
            "unsupported OCI Tuff pack layer media type: {}",
            layer.media_type
        )));
    }
    if layer.size < 0 || !valid_sha256_digest(&layer.digest) {
        return Err(TuffError::new(
            "OCI Tuff pack layer has an invalid size or SHA-256 digest",
        ));
    }
    Ok(())
}

fn validate_pack_annotations(
    manifest: &OciImageManifest,
    artifact: &pack::PackArtifact,
) -> Result<()> {
    let annotations = manifest
        .annotations
        .as_ref()
        .ok_or_else(|| TuffError::new("OCI Tuff pack manifest is missing annotations"))?;
    for (key, expected) in [
        (OCI_TITLE_ANNOTATION, artifact.metadata.name.as_str()),
        (OCI_VERSION_ANNOTATION, artifact.metadata.version.as_str()),
        (
            OCI_DESCRIPTION_ANNOTATION,
            artifact.metadata.description.as_str(),
        ),
    ] {
        if annotations.get(key).map(String::as_str) != Some(expected) {
            return Err(TuffError::new(format!(
                "OCI manifest annotation '{key}' does not match the Tuff pack metadata"
            )));
        }
    }
    Ok(())
}

fn registry_client(options: &OciTransferOptions) -> Result<Client> {
    let mut certificates = Vec::with_capacity(options.ca_files.len());
    for path in &options.ca_files {
        let data = fs::read(path).map_err(|error| {
            TuffError::new(format!(
                "could not read OCI certificate authority {}: {error}",
                path.display()
            ))
        })?;
        certificates.push(Certificate {
            encoding: CertificateEncoding::Pem,
            data,
        });
    }
    Client::try_from(ClientConfig {
        protocol: if options.plain_http {
            ClientProtocol::Http
        } else {
            ClientProtocol::Https
        },
        extra_root_certificates: certificates,
        platform_resolver: None,
        ..Default::default()
    })
    .map_err(|error| oci_error("create OCI registry client", error))
}

fn registry_auth(registry: &str) -> Result<RegistryAuth> {
    if docker_config_path().is_some_and(|path| path.is_file()) {
        match docker_credential::get_credential(registry) {
            Ok(credential) => return Ok(convert_credential(credential)),
            Err(CredentialRetrievalError::NoCredentialConfigured) => {}
            Err(error) => return Err(credential_error("Docker", error)),
        }
    }
    if podman_config_path().is_some_and(|path| path.is_file()) {
        match docker_credential::get_podman_credential(registry) {
            Ok(credential) => return Ok(convert_credential(credential)),
            Err(CredentialRetrievalError::NoCredentialConfigured) => {}
            Err(error) => return Err(credential_error("Podman", error)),
        }
    }
    Ok(RegistryAuth::Anonymous)
}

fn convert_credential(credential: DockerCredential) -> RegistryAuth {
    match credential {
        DockerCredential::IdentityToken(token) => RegistryAuth::Bearer(token),
        DockerCredential::UsernamePassword(username, password) => {
            RegistryAuth::Basic(username, password)
        }
    }
}

fn credential_error(source: &str, error: CredentialRetrievalError) -> TuffError {
    let detail = match error {
        CredentialRetrievalError::HelperCommunicationError => {
            "could not communicate with the configured credential helper".to_string()
        }
        CredentialRetrievalError::MalformedHelperResponse => {
            "the configured credential helper returned a malformed response".to_string()
        }
        CredentialRetrievalError::HelperFailure { helper, .. } => {
            format!("credential helper '{helper}' failed")
        }
        CredentialRetrievalError::CredentialDecodingError => {
            "the stored credential could not be decoded".to_string()
        }
        CredentialRetrievalError::CredentialMismatchError => {
            "the stored credential fields do not agree".to_string()
        }
        CredentialRetrievalError::NoCredentialConfigured => {
            "no credential is configured".to_string()
        }
        CredentialRetrievalError::ConfigNotFound => {
            "the credential configuration was not found".to_string()
        }
        CredentialRetrievalError::ConfigReadError => {
            "the credential configuration could not be read".to_string()
        }
    };
    TuffError::new(format!(
        "could not load {source} registry credentials: {detail}; run `{}` login for the registry and try again",
        source.to_ascii_lowercase()
    ))
}

fn docker_config_path() -> Option<PathBuf> {
    env::var_os("DOCKER_CONFIG")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".docker")))
        .map(|directory| directory.join("config.json"))
}

fn podman_config_path() -> Option<PathBuf> {
    if let Some(path) = env::var_os("REGISTRY_AUTH_FILE") {
        return Some(PathBuf::from(path));
    }
    let primary = if cfg!(target_os = "linux") {
        env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .map(|path| path.join("containers/auth.json"))
    } else {
        env::var_os("HOME")
            .map(PathBuf::from)
            .map(|path| path.join(".config/containers/auth.json"))
    };
    if primary.as_ref().is_some_and(|path| path.is_file()) {
        return primary;
    }
    env::var_os("DOCKER_CONFIG")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".docker")))
        .map(|directory| directory.join("containers/auth.json"))
}

async fn push_blob_if_missing(
    client: &Client,
    reference: &Reference,
    bytes: &[u8],
    digest: &str,
) -> Result<()> {
    if !client
        .blob_exists(reference, digest)
        .await
        .map_err(|error| oci_error("check OCI blob", error))?
    {
        client
            .push_blob(reference, bytes.to_vec(), digest)
            .await
            .map_err(|error| oci_error("push OCI blob", error))?;
    }
    Ok(())
}

fn manifest_is_missing(error: &OciDistributionError) -> bool {
    match error {
        OciDistributionError::ImageManifestNotFoundError(_)
        | OciDistributionError::ServerError { code: 404, .. } => true,
        OciDistributionError::RegistryError { envelope, .. } => {
            envelope.errors.iter().any(|item| {
                matches!(
                    item.code,
                    OciErrorCode::ManifestUnknown
                        | OciErrorCode::NameUnknown
                        | OciErrorCode::NotFound
                )
            })
        }
        _ => false,
    }
}

fn oci_error(action: &str, error: OciDistributionError) -> TuffError {
    TuffError::new(format!("could not {action}: {error}"))
}

fn digest_reference(reference: &Reference, digest: &str) -> String {
    format!(
        "{}/{}@{digest}",
        reference.registry(),
        reference.repository()
    )
}

fn sha256_digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn valid_sha256_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64 && digest.chars().all(|item| item.is_ascii_hexdigit())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::PackArtifactMetadata;

    fn artifact() -> pack::PackArtifact {
        pack::PackArtifact {
            metadata: PackArtifactMetadata {
                artifact_version: pack::PACK_ARTIFACT_VERSION,
                pack_schema: pack::PACK_SCHEMA_VERSION,
                name: "com.acme/engineering".into(),
                version: "1.2.0".into(),
                description: "Acme engineering capabilities.".into(),
                capabilities: Vec::new(),
                targets: Vec::new(),
                files: Vec::new(),
            },
            contents: Vec::new(),
            digest: "a".repeat(64),
        }
    }

    #[test]
    fn push_reference_requires_explicit_tag() {
        let error = parse_push_reference("ghcr.io/acme/engineering").unwrap_err();
        assert!(error.to_string().contains("explicit tag"));
    }

    #[test]
    fn push_reference_rejects_digest() {
        let reference = format!("ghcr.io/acme/engineering@sha256:{}", "a".repeat(64));
        let error = parse_push_reference(&reference).unwrap_err();
        assert!(error.to_string().contains("explicit tag"));
    }

    #[test]
    fn pull_reference_requires_explicit_tag_or_digest() {
        let error = parse_pull_reference("ghcr.io/acme/engineering").unwrap_err();
        assert!(error.to_string().contains("implicit 'latest'"));
    }

    #[test]
    fn references_accept_registry_ports() {
        assert!(parse_push_reference("localhost:5000/acme/engineering:1.2.0").is_ok());
    }

    #[test]
    fn identity_token_becomes_bearer_auth() {
        let auth = convert_credential(DockerCredential::IdentityToken("secret".into()));
        assert_eq!(auth, RegistryAuth::Bearer("secret".into()));
    }

    #[test]
    fn credential_helper_error_does_not_include_helper_output() {
        let error = credential_error(
            "Docker",
            CredentialRetrievalError::HelperFailure {
                helper: "test".into(),
                stdout: "sensitive-stdout".into(),
                stderr: "sensitive-stderr".into(),
            },
        );
        let message = error.to_string();
        assert!(!message.contains("sensitive"));
    }

    #[test]
    fn sha256_validation_requires_prefixed_lower_or_upper_hex() {
        assert!(valid_sha256_digest(&format!("sha256:{}", "a".repeat(64))));
        assert!(!valid_sha256_digest(&format!("sha512:{}", "a".repeat(64))));
    }

    #[test]
    fn manifest_is_deterministic_and_contains_one_pack_layer() {
        let artifact = artifact();
        let digest = format!("sha256:{}", artifact.digest);
        let left = serde_json::to_vec(&pack_manifest(&artifact, &digest, 42).unwrap()).unwrap();
        let right = serde_json::to_vec(&pack_manifest(&artifact, &digest, 42).unwrap()).unwrap();

        assert_eq!(left, right);
        let manifest: OciImageManifest = serde_json::from_slice(&left).unwrap();
        assert_eq!(
            manifest.artifact_type.as_deref(),
            Some(PACK_ARTIFACT_MEDIA_TYPE)
        );
        assert_eq!(manifest.layers.len(), 1);
        assert_eq!(manifest.layers[0].digest, digest);
    }

    #[test]
    fn manifest_validation_rejects_extra_layers() {
        let artifact = artifact();
        let digest = format!("sha256:{}", artifact.digest);
        let mut manifest = pack_manifest(&artifact, &digest, 42).unwrap();
        manifest.layers.push(manifest.layers[0].clone());

        let error = validate_pack_manifest(&manifest).unwrap_err();
        assert!(error.to_string().contains("exactly one layer"));
    }

    #[test]
    fn manifest_validation_rejects_wrong_artifact_type() {
        let artifact = artifact();
        let digest = format!("sha256:{}", artifact.digest);
        let mut manifest = pack_manifest(&artifact, &digest, 42).unwrap();
        manifest.artifact_type = Some("application/vnd.example.other.v1".into());

        let error = validate_pack_manifest(&manifest).unwrap_err();
        assert!(error.to_string().contains("not a Tuff pack"));
    }

    #[test]
    fn manifest_validation_rejects_wrong_layer_media_type() {
        let artifact = artifact();
        let digest = format!("sha256:{}", artifact.digest);
        let mut manifest = pack_manifest(&artifact, &digest, 42).unwrap();
        manifest.layers[0].media_type = "application/octet-stream".into();

        let error = validate_pack_manifest(&manifest).unwrap_err();
        assert!(error.to_string().contains("layer media type"));
    }

    #[test]
    fn manifest_validation_rejects_non_empty_config_contract() {
        let artifact = artifact();
        let digest = format!("sha256:{}", artifact.digest);
        let mut manifest = pack_manifest(&artifact, &digest, 42).unwrap();
        manifest.config.size = 0;

        let error = validate_pack_manifest(&manifest).unwrap_err();
        assert!(error.to_string().contains("empty configuration"));
    }

    #[test]
    fn manifest_validation_rejects_subject_on_primary_pack() {
        let artifact = artifact();
        let digest = format!("sha256:{}", artifact.digest);
        let mut manifest = pack_manifest(&artifact, &digest, 42).unwrap();
        manifest.subject = Some(manifest.layers[0].clone());

        let error = validate_pack_manifest(&manifest).unwrap_err();
        assert!(error.to_string().contains("must not declare a subject"));
    }

    #[test]
    fn annotation_validation_rejects_metadata_mismatch() {
        let artifact = artifact();
        let digest = format!("sha256:{}", artifact.digest);
        let mut manifest = pack_manifest(&artifact, &digest, 42).unwrap();
        manifest
            .annotations
            .as_mut()
            .unwrap()
            .insert(OCI_VERSION_ANNOTATION.into(), "9.9.9".into());

        let error = validate_pack_annotations(&manifest, &artifact).unwrap_err();
        assert!(error.to_string().contains(OCI_VERSION_ANNOTATION));
    }
}
