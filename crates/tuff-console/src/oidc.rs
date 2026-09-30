//! Publishing from GitHub Actions without a secret (RFC-108 D5).
//!
//! A job asks the runner for an OIDC token whose audience is the console's
//! URL and sends it as `Authorization: Bearer`. [`Verifier::verify`] checks
//! the token's signature against the issuer's published keys, its issuer,
//! audience, and validity window, and that the repository's owner is one the
//! console trusts. The caller then binds the report to the token's
//! `repository` claim.

use std::str::FromStr;
use std::time::{Duration, Instant};

use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use tuff_core::error::{Result, TuffError};

/// `iss` of a GitHub Actions token.
pub const GITHUB_ISSUER: &str = "https://token.actions.githubusercontent.com";

/// Where GitHub publishes the keys that sign its tokens.
pub const GITHUB_JWKS_URL: &str = "https://token.actions.githubusercontent.com/.well-known/jwks";

/// Allowed clock difference in either direction for `exp` and `nbf`.
const LEEWAY_SECONDS: u64 = 60;

/// How long the cached keys are trusted to be complete before an unknown
/// `kid` triggers another fetch. Bounds the requests a forged `kid` can cause.
const DEFAULT_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

/// A source of tokens the console accepts: `github:<owner>`. The provider
/// prefix is kept so GitHub Enterprise Server and GitLab can be added
/// without changing how trusts are stored or shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Trust {
    pub provider: String,
    pub owner: String,
}

impl Trust {
    /// The repository host a provider's `repository` claim belongs to.
    pub fn host(&self) -> &'static str {
        "github.com"
    }
}

impl FromStr for Trust {
    type Err = TuffError;

    fn from_str(text: &str) -> Result<Self> {
        let Some((provider, owner)) = text.split_once(':') else {
            return Err(TuffError::usage(format!(
                "'{text}' is not a trust: expected <provider>:<owner>"
            ))
            .with_hint("for example --trust github:acme"));
        };
        if provider != "github" {
            return Err(TuffError::unsupported(format!(
                "trust provider '{provider}' is not supported"
            ))
            .with_hint("only 'github' is supported, as --trust github:<owner>"));
        }
        let valid = !owner.is_empty()
            && owner.len() <= 100
            && owner
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
        if !valid {
            return Err(
                TuffError::usage(format!("'{owner}' is not a valid GitHub owner name"))
                    .with_hint("use the organisation or user name, such as --trust github:acme"),
            );
        }
        Ok(Self {
            provider: provider.to_string(),
            owner: owner.to_string(),
        })
    }
}

impl std::fmt::Display for Trust {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.provider, self.owner)
    }
}

/// Why a token was not accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OidcError {
    /// The token is malformed, unsigned by the issuer, expired, or meant for
    /// another audience.
    Invalid(String),
    /// The token is genuine, but its repository owner is not trusted.
    Untrusted(String),
    /// The issuer's keys could not be fetched.
    Unavailable(String),
}

/// The claims of an accepted token the console uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedToken {
    /// `owner/name`, as GitHub writes the `repository` claim.
    pub repository: String,
    pub owner: String,
    /// The repository as a report names it: host, owner, and name.
    pub report_repository: String,
}

#[derive(Debug, Deserialize)]
struct Claims {
    repository: String,
    repository_owner: String,
}

#[derive(Default)]
struct KeyCache {
    set: Option<JwkSet>,
    fetched_at: Option<Instant>,
}

/// Verifies GitHub Actions tokens for one console.
pub struct Verifier {
    trusts: Vec<Trust>,
    issuer: String,
    jwks_url: String,
    audiences: [String; 2],
    refresh_interval: Duration,
    client: reqwest::Client,
    cache: tokio::sync::Mutex<KeyCache>,
}

impl Verifier {
    /// Trust `trusts` for tokens whose audience is `audience`, the console's
    /// public URL. A trailing slash on the URL does not matter.
    pub fn new(trusts: Vec<Trust>, audience: &str) -> Result<Self> {
        let audience = audience.trim_end_matches('/');
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|error| {
                TuffError::source_failed(format!("cannot build an HTTP client: {error}"))
            })?;
        Ok(Self {
            trusts,
            issuer: GITHUB_ISSUER.to_string(),
            jwks_url: GITHUB_JWKS_URL.to_string(),
            audiences: [audience.to_string(), format!("{audience}/")],
            refresh_interval: DEFAULT_REFRESH_INTERVAL,
            client,
            cache: tokio::sync::Mutex::new(KeyCache::default()),
        })
    }

    /// Accept tokens from another issuer, such as GitHub Enterprise Server
    /// or a test double.
    pub fn with_issuer(mut self, issuer: &str, jwks_url: &str) -> Self {
        self.issuer = issuer.to_string();
        self.jwks_url = jwks_url.to_string();
        self
    }

    /// Change how long fetched keys are kept before an unknown `kid`
    /// fetches them again.
    pub fn with_refresh_interval(mut self, interval: Duration) -> Self {
        self.refresh_interval = interval;
        self
    }

    pub fn trusts(&self) -> &[Trust] {
        &self.trusts
    }

    pub fn audience(&self) -> &str {
        &self.audiences[0]
    }

    pub async fn verify(&self, token: &str) -> std::result::Result<VerifiedToken, OidcError> {
        let header = jsonwebtoken::decode_header(token)
            .map_err(|error| OidcError::Invalid(format!("the token is not a JWT: {error}")))?;
        if header.alg != Algorithm::RS256 {
            return Err(OidcError::Invalid(format!(
                "the token is signed with {:?}, and only RS256 is accepted",
                header.alg
            )));
        }
        let kid = header
            .kid
            .ok_or_else(|| OidcError::Invalid("the token names no signing key".to_string()))?;
        let key = self.key_for(&kid).await?;

        let mut validation = Validation::new(Algorithm::RS256);
        validation.leeway = LEEWAY_SECONDS;
        validation.validate_nbf = true;
        validation.set_issuer(&[self.issuer.as_str()]);
        validation.set_audience(&self.audiences);
        validation.set_required_spec_claims(&["exp", "iss", "aud"]);
        let data = jsonwebtoken::decode::<Claims>(token, &key, &validation)
            .map_err(|error| OidcError::Invalid(format!("the token is not valid: {error}")))?;
        let claims = data.claims;

        let trust = self
            .trusts
            .iter()
            .find(|trust| trust.owner.eq_ignore_ascii_case(&claims.repository_owner))
            .ok_or_else(|| {
                OidcError::Untrusted(format!(
                    "this console does not trust GitHub owner '{}'",
                    claims.repository_owner
                ))
            })?;
        Ok(VerifiedToken {
            report_repository: format!("{}/{}", trust.host(), claims.repository),
            repository: claims.repository,
            owner: claims.repository_owner,
        })
    }

    async fn key_for(&self, kid: &str) -> std::result::Result<DecodingKey, OidcError> {
        let mut cache = self.cache.lock().await;
        if let Some(key) = find_key(cache.set.as_ref(), kid)? {
            return Ok(key);
        }
        let stale = cache
            .fetched_at
            .is_none_or(|fetched| fetched.elapsed() >= self.refresh_interval);
        if stale {
            let set = self.fetch().await?;
            cache.set = Some(set);
            cache.fetched_at = Some(Instant::now());
        }
        find_key(cache.set.as_ref(), kid)?.ok_or_else(|| {
            OidcError::Invalid(format!("the issuer publishes no signing key '{kid}'"))
        })
    }

    async fn fetch(&self) -> std::result::Result<JwkSet, OidcError> {
        let unavailable = |reason: String| {
            OidcError::Unavailable(format!(
                "cannot fetch the signing keys from {}: {reason}",
                self.jwks_url
            ))
        };
        let response = self
            .client
            .get(&self.jwks_url)
            .send()
            .await
            .map_err(|error| unavailable(error.to_string()))?;
        if !response.status().is_success() {
            return Err(unavailable(format!("HTTP {}", response.status())));
        }
        response
            .json::<JwkSet>()
            .await
            .map_err(|error| unavailable(error.to_string()))
    }
}

fn find_key(
    set: Option<&JwkSet>,
    kid: &str,
) -> std::result::Result<Option<DecodingKey>, OidcError> {
    let Some(jwk) = set.and_then(|set| set.find(kid)) else {
        return Ok(None);
    };
    DecodingKey::from_jwk(jwk)
        .map(Some)
        .map_err(|error| OidcError::Invalid(format!("signing key '{kid}' is unusable: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuff_core::error::ErrorKind;

    #[test]
    fn trusts_parse_with_a_provider_prefix() {
        let trust: Trust = "github:acme".parse().unwrap();
        assert_eq!(trust.provider, "github");
        assert_eq!(trust.owner, "acme");
        assert_eq!(trust.to_string(), "github:acme");
        assert_eq!(trust.host(), "github.com");
    }

    #[test]
    fn a_malformed_trust_is_refused_with_a_hint() {
        for bad in ["acme", "github:", "github:a/b", "github:has space"] {
            let error = bad.parse::<Trust>().unwrap_err();
            assert_eq!(error.kind(), ErrorKind::Usage, "{bad}");
            assert!(error.hint().unwrap().contains("github:"), "{bad}");
        }
        let error = "gitlab:acme".parse::<Trust>().unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Unsupported);
    }
}
