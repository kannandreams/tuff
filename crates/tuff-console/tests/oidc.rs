//! Publishing with a GitHub Actions OIDC token, against a key set served
//! from the test itself. Nothing here reaches the network.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aws_lc_rs::encoding::{AsDer, Pkcs8V1Der};
use aws_lc_rs::rsa::{KeyPair as RsaKeyPair, KeySize, PublicKeyComponents};
use aws_lc_rs::signature::KeyPair;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::{Value, json};
use tuff_console::{ServerOptions, Store, Trust, Verifier, serve};

const ISSUER: &str = "https://issuer.test";
const AUDIENCE: &str = "http://console.test";

/// jsonwebtoken signs with a PKCS#1 key, and aws-lc-rs exports PKCS#8, which
/// wraps the PKCS#1 key in an OCTET STRING after the rsaEncryption
/// algorithm identifier.
fn pkcs1_from_pkcs8(pkcs8: &[u8]) -> Vec<u8> {
    const ALGORITHM: [u8; 15] = [
        0x30, 0x0d, 0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01, 0x05, 0x00,
    ];
    let at = pkcs8
        .windows(ALGORITHM.len())
        .position(|window| window == ALGORITHM)
        .unwrap()
        + ALGORITHM.len();
    assert_eq!(pkcs8[at], 0x04, "an OCTET STRING follows");
    let length_bytes = match pkcs8[at + 1] {
        0x82 => 2,
        0x81 => 1,
        _ => panic!("unexpected DER length"),
    };
    let length = pkcs8[at + 2..at + 2 + length_bytes]
        .iter()
        .fold(0usize, |total, byte| (total << 8) | usize::from(*byte));
    let start = at + 2 + length_bytes;
    pkcs8[start..start + length].to_vec()
}

/// An RSA key that signs tokens and publishes itself as a JWK.
struct Signer {
    kid: String,
    der: Vec<u8>,
    jwk: Value,
}

impl Signer {
    fn new(kid: &str) -> Self {
        let key = RsaKeyPair::generate(KeySize::Rsa2048).unwrap();
        let pkcs8 = AsDer::<Pkcs8V1Der>::as_der(&key).unwrap();
        let der = pkcs1_from_pkcs8(pkcs8.as_ref());
        let components: PublicKeyComponents<Vec<u8>> = key.public_key().into();
        let jwk = json!({
            "kty": "RSA",
            "use": "sig",
            "alg": "RS256",
            "kid": kid,
            "n": URL_SAFE_NO_PAD.encode(&components.n),
            "e": URL_SAFE_NO_PAD.encode(&components.e),
        });
        Self {
            kid: kid.to_string(),
            der,
            jwk,
        }
    }

    fn sign(&self, claims: &Value) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.kid.clone());
        jsonwebtoken::encode(&header, claims, &EncodingKey::from_rsa_der(&self.der)).unwrap()
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn claims(owner: &str, repository: &str) -> Value {
    json!({
        "iss": ISSUER,
        "aud": AUDIENCE,
        "iat": now(),
        "nbf": now() - 5,
        "exp": now() + 300,
        "repository": repository,
        "repository_owner": owner,
    })
}

/// The issuer's key set over HTTP, changeable mid-test, counting fetches.
struct FakeIssuer {
    url: String,
    keys: Arc<Mutex<Vec<Value>>>,
    fetches: Arc<AtomicUsize>,
}

impl FakeIssuer {
    async fn start(keys: Vec<Value>) -> Self {
        let keys = Arc::new(Mutex::new(keys));
        let fetches = Arc::new(AtomicUsize::new(0));
        let app = axum::Router::new().route(
            "/jwks",
            axum::routing::get({
                let keys = Arc::clone(&keys);
                let fetches = Arc::clone(&fetches);
                move || {
                    let keys = Arc::clone(&keys);
                    let fetches = Arc::clone(&fetches);
                    async move {
                        fetches.fetch_add(1, Ordering::SeqCst);
                        axum::Json(json!({ "keys": *keys.lock().unwrap() }))
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/jwks", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self { url, keys, fetches }
    }
}

struct Console {
    base: String,
    store: Arc<Store>,
    _data: tempfile::TempDir,
}

impl Console {
    async fn start(issuer: &FakeIssuer, refresh: Duration) -> Self {
        let verifier = Verifier::new(vec!["github:acme".parse::<Trust>().unwrap()], AUDIENCE)
            .unwrap()
            .with_issuer(ISSUER, &issuer.url)
            .with_refresh_interval(refresh);
        let data = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(data.path()).unwrap());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn({
            let store = Arc::clone(&store);
            async move {
                serve(
                    store,
                    listener,
                    ServerOptions {
                        require_key: false,
                        oidc: Some(Arc::new(verifier)),
                        ..Default::default()
                    },
                    std::future::pending(),
                )
                .await
                .unwrap();
            }
        });
        Self {
            base,
            store,
            _data: data,
        }
    }

    async fn publish(&self, token: Option<&str>, report: &Value) -> (u16, Value) {
        let mut request = reqwest::Client::new()
            .post(format!("{}/api/v1/reports", self.base))
            .json(report);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        (status, response.json().await.unwrap())
    }
}

fn report(dir: &Path, repository: &str) -> Value {
    std::fs::write(dir.join("tuff.lock"), "{\"version\":3,\"capabilities\":[]}").unwrap();
    let report = tuff_core::report::build_report(dir, Some(repository), None).unwrap();
    serde_json::to_value(report).unwrap()
}

const LONG: Duration = Duration::from_secs(3600);

#[tokio::test]
async fn a_valid_token_publishes_for_its_own_repository() {
    let signer = Signer::new("k1");
    let issuer = FakeIssuer::start(vec![signer.jwk.clone()]).await;
    let console = Console::start(&issuer, LONG).await;
    let project = tempfile::tempdir().unwrap();
    let report = report(project.path(), "github.com/acme/web");

    let token = signer.sign(&claims("acme", "acme/web"));
    let (status, outcome) = console.publish(Some(&token), &report).await;
    assert_eq!(status, 201, "{outcome}");
    assert_eq!(outcome["projectFirstSeen"], true);
    assert_eq!(console.store.projects().unwrap().len(), 1);

    // The keys are fetched once and cached for the next token.
    let (status, outcome) = console.publish(Some(&token), &report).await;
    assert_eq!(status, 200, "{outcome}");
    assert_eq!(issuer.fetches.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn owner_and_repository_names_compare_without_case() {
    let signer = Signer::new("k1");
    let issuer = FakeIssuer::start(vec![signer.jwk.clone()]).await;
    let console = Console::start(&issuer, LONG).await;
    let project = tempfile::tempdir().unwrap();
    let report = report(project.path(), "GitHub.com/ACME/Web");

    let token = signer.sign(&claims("Acme", "acme/web"));
    let (status, outcome) = console.publish(Some(&token), &report).await;
    assert_eq!(status, 201, "{outcome}");
}

#[tokio::test]
async fn a_token_of_an_untrusted_owner_is_refused() {
    let signer = Signer::new("k1");
    let issuer = FakeIssuer::start(vec![signer.jwk.clone()]).await;
    let console = Console::start(&issuer, LONG).await;
    let project = tempfile::tempdir().unwrap();
    let report = report(project.path(), "github.com/mallory/web");

    let token = signer.sign(&claims("mallory", "mallory/web"));
    let (status, error) = console.publish(Some(&token), &report).await;
    assert_eq!(status, 403);
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("mallory")
    );
    assert!(console.store.projects().unwrap().is_empty());
}

#[tokio::test]
async fn a_token_for_another_audience_is_refused() {
    let signer = Signer::new("k1");
    let issuer = FakeIssuer::start(vec![signer.jwk.clone()]).await;
    let console = Console::start(&issuer, LONG).await;
    let project = tempfile::tempdir().unwrap();
    let report = report(project.path(), "github.com/acme/web");

    let mut wrong = claims("acme", "acme/web");
    wrong["aud"] = json!("https://other.example");
    let (status, error) = console.publish(Some(&signer.sign(&wrong)), &report).await;
    assert_eq!(status, 401);
    assert!(error["error"]["hint"].as_str().unwrap().contains(AUDIENCE));

    // A trailing slash on the console URL is the same audience.
    let mut slash = claims("acme", "acme/web");
    slash["aud"] = json!(format!("{AUDIENCE}/"));
    let (status, _) = console.publish(Some(&signer.sign(&slash)), &report).await;
    assert_eq!(status, 201);
}

#[tokio::test]
async fn an_expired_or_not_yet_valid_token_is_refused() {
    let signer = Signer::new("k1");
    let issuer = FakeIssuer::start(vec![signer.jwk.clone()]).await;
    let console = Console::start(&issuer, LONG).await;
    let project = tempfile::tempdir().unwrap();
    let report = report(project.path(), "github.com/acme/web");

    let mut expired = claims("acme", "acme/web");
    expired["exp"] = json!(now() - 3600);
    let (status, _) = console.publish(Some(&signer.sign(&expired)), &report).await;
    assert_eq!(status, 401);

    let mut early = claims("acme", "acme/web");
    early["nbf"] = json!(now() + 3600);
    let (status, _) = console.publish(Some(&signer.sign(&early)), &report).await;
    assert_eq!(status, 401);

    // A few seconds of clock skew is allowed.
    let mut skewed = claims("acme", "acme/web");
    skewed["exp"] = json!(now() - 10);
    let (status, _) = console.publish(Some(&signer.sign(&skewed)), &report).await;
    assert_eq!(status, 201);
}

#[tokio::test]
async fn a_token_cannot_publish_for_another_repository() {
    let signer = Signer::new("k1");
    let issuer = FakeIssuer::start(vec![signer.jwk.clone()]).await;
    let console = Console::start(&issuer, LONG).await;
    let project = tempfile::tempdir().unwrap();
    let report = report(project.path(), "github.com/acme/other");

    let token = signer.sign(&claims("acme", "acme/web"));
    let (status, error) = console.publish(Some(&token), &report).await;
    assert_eq!(status, 403);
    let message = error["error"]["message"].as_str().unwrap();
    assert!(message.contains("github.com/acme/web"), "{message}");
    assert!(message.contains("github.com/acme/other"), "{message}");
    assert!(console.store.projects().unwrap().is_empty());
}

#[tokio::test]
async fn a_token_signed_by_another_key_is_refused() {
    let signer = Signer::new("k1");
    let forger = Signer::new("k1");
    let issuer = FakeIssuer::start(vec![signer.jwk.clone()]).await;
    let console = Console::start(&issuer, LONG).await;
    let project = tempfile::tempdir().unwrap();
    let report = report(project.path(), "github.com/acme/web");

    let token = forger.sign(&claims("acme", "acme/web"));
    let (status, _) = console.publish(Some(&token), &report).await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn an_unknown_key_id_is_refused_and_does_not_refetch_at_once() {
    let signer = Signer::new("k1");
    let rotated = Signer::new("k2");
    let issuer = FakeIssuer::start(vec![signer.jwk.clone()]).await;
    let console = Console::start(&issuer, LONG).await;
    let project = tempfile::tempdir().unwrap();
    let report = report(project.path(), "github.com/acme/web");

    let (status, _) = console
        .publish(Some(&signer.sign(&claims("acme", "acme/web"))), &report)
        .await;
    assert_eq!(status, 201);
    assert_eq!(issuer.fetches.load(Ordering::SeqCst), 1);

    issuer.keys.lock().unwrap().push(rotated.jwk.clone());
    let token = rotated.sign(&claims("acme", "acme/web"));
    let (status, error) = console.publish(Some(&token), &report).await;
    assert_eq!(status, 401);
    assert!(error["error"]["message"].as_str().unwrap().contains("k2"));
    assert_eq!(
        issuer.fetches.load(Ordering::SeqCst),
        1,
        "a forged kid must not make the console fetch keys on every request"
    );
}

#[tokio::test]
async fn a_rotated_key_is_found_by_refreshing_the_key_set() {
    let signer = Signer::new("k1");
    let rotated = Signer::new("k2");
    let issuer = FakeIssuer::start(vec![signer.jwk.clone()]).await;
    let console = Console::start(&issuer, Duration::ZERO).await;
    let project = tempfile::tempdir().unwrap();
    let report = report(project.path(), "github.com/acme/web");

    let (status, _) = console
        .publish(Some(&signer.sign(&claims("acme", "acme/web"))), &report)
        .await;
    assert_eq!(status, 201);

    issuer.keys.lock().unwrap().push(rotated.jwk.clone());
    let token = rotated.sign(&claims("acme", "acme/web"));
    let (status, outcome) = console.publish(Some(&token), &report).await;
    assert_eq!(status, 200, "{outcome}");
    assert_eq!(issuer.fetches.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn an_unreachable_issuer_is_a_503_and_not_a_login_failure() {
    let signer = Signer::new("k1");
    // A closed port stands in for an issuer that is down.
    let dead = FakeIssuer {
        url: "http://127.0.0.1:1/jwks".to_string(),
        keys: Arc::default(),
        fetches: Arc::default(),
    };
    let console = Console::start(&dead, LONG).await;
    let project = tempfile::tempdir().unwrap();
    let report = report(project.path(), "github.com/acme/web");
    let (status, _) = console
        .publish(Some(&signer.sign(&claims("acme", "acme/web"))), &report)
        .await;
    assert_eq!(status, 503);
}

#[tokio::test]
async fn a_configured_trust_makes_publishing_authenticate() {
    let signer = Signer::new("k1");
    let issuer = FakeIssuer::start(vec![signer.jwk.clone()]).await;
    let console = Console::start(&issuer, LONG).await;
    let project = tempfile::tempdir().unwrap();
    let report = report(project.path(), "github.com/acme/web");

    let (status, error) = console.publish(None, &report).await;
    assert_eq!(status, 401);
    assert!(
        error["error"]["hint"]
            .as_str()
            .unwrap()
            .contains("GitHub Actions")
    );
    let (status, _) = console.publish(Some("not.a.jwt"), &report).await;
    assert_eq!(status, 401);

    // Keys keep working beside a trust.
    let key = console.store.create_key("ci", None).unwrap();
    let (status, _) = console.publish(Some(&key), &report).await;
    assert_eq!(status, 201);
}

#[tokio::test]
async fn a_scoped_key_publishes_only_for_its_repository() {
    let issuer = FakeIssuer::start(vec![]).await;
    let console = Console::start(&issuer, LONG).await;
    let project = tempfile::tempdir().unwrap();
    let web = report(project.path(), "github.com/acme/web");
    let other = {
        let second = tempfile::tempdir().unwrap();
        report(second.path(), "github.com/acme/other")
    };

    let key = console
        .store
        .create_key("web-ci", Some("https://github.com/Acme/web.git"))
        .unwrap();
    let (status, _) = console.publish(Some(&key), &web).await;
    assert_eq!(status, 201);
    let (status, error) = console.publish(Some(&key), &other).await;
    assert_eq!(status, 403);
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("github.com/Acme/web")
    );

    let open = console.store.create_key("any", None).unwrap();
    let (status, _) = console.publish(Some(&open), &other).await;
    assert_eq!(status, 201);
}
