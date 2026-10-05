//! GoonBox (goonbox.cr) authentication.
//!
//! GoonBox exposes a public read-only API under `/api/v1` and refuses uploads
//! there (`upload_disabled`), so the only way to write is `/api/upload`, which
//! authenticates against a Sanctum bearer token. Obtaining that token is the
//! awkward part: `POST /api/auth/login` verifies a Cloudflare Turnstile token
//! before it looks at credentials at all, so a stale or missing captcha token
//! returns 422 regardless of the password.
//!
//! The bearer is long-lived enough to serve a whole batch, so it is cached and
//! the captcha is paid for at most once per uploader.

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, ORIGIN, REFERER, USER_AGENT};
use reqwest::Client;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use upio_core::error::UploadError;

pub const BASE_URL: &str = "https://goonbox.cr";
pub const LOGIN_URL: &str = "https://goonbox.cr/login";
const CSRF_URL: &str = "https://goonbox.cr/sanctum/csrf-cookie";
const LOGIN_API_URL: &str = "https://goonbox.cr/api/auth/login";
const XSRF_COOKIE: &str = "XSRF-TOKEN";
pub(crate) const XSRF_HEADER: &str = "X-XSRF-TOKEN";

const DESKTOP_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                                 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";

/// Cloudflare 403s anything that does not look like a browser, serving a
/// "Just a moment..." interstitial instead of JSON. These headers are what get
/// a plain HTTP client through; without them every request fails before it
/// reaches Laravel.
pub fn browser_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/json, text/plain, */*"),
    );
    headers.insert(
        HeaderName::from_static("accept-language"),
        HeaderValue::from_static("en-US,en;q=0.9"),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-dest"),
        HeaderValue::from_static("empty"),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-mode"),
        HeaderValue::from_static("cors"),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-site"),
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(USER_AGENT, HeaderValue::from_static(DESKTOP_USER_AGENT));
    headers.insert(ORIGIN, HeaderValue::from_static(BASE_URL));
    headers.insert(
        REFERER,
        HeaderValue::from_static("https://goonbox.cr/upload"),
    );
    headers
}

pub struct Credentials {
    pub username: String,
    pub password: String,
    /// Set by a solved challenge. Never read from config: a captcha token is
    /// single-purpose and short-lived, so persisting one would just be a stale
    /// value that fails the next login.
    pub solved_captcha: RwLock<Option<String>>,
    pub profile_dir: PathBuf,
}

#[derive(Deserialize)]
struct LoginResponse {
    token: Option<String>,
}

pub type Bearer = Arc<str>;

pub struct Session {
    client: Client,
    credentials: Credentials,
    /// A `OnceCell` cannot be cleared, and a bearer that expired mid-batch has
    /// to be replaceable, so this is a lock rather than a cell.
    bearer: RwLock<Option<Bearer>>,
}

impl Session {
    pub fn new(credentials: Credentials) -> Result<Self, UploadError> {
        let client = Client::builder()
            .default_headers(browser_headers())
            .cookie_store(true)
            .build()
            .map_err(|e| UploadError {
                message: format!("could not build the goonbox HTTP client: {e}"),
                status_code: None,
            })?;

        Ok(Self {
            client,
            credentials,
            bearer: RwLock::new(None),
        })
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    /// A session with no credentials, for when building the client failed. Every
    /// request it makes fails, which keeps the failure a normal `UploadError`
    /// at upload time instead of a panic in a constructor.
    pub fn new_fallback() -> Self {
        Self {
            client: Client::new(),
            credentials: Credentials {
                username: String::new(),
                password: String::new(),
                solved_captcha: RwLock::new(None),
                profile_dir: std::env::temp_dir(),
            },
            bearer: RwLock::new(None),
        }
    }

    pub async fn bearer(&self) -> Result<Bearer, UploadError> {
        if let Some(existing) = self.bearer.read().await.clone() {
            return Ok(existing);
        }
        self.login().await
    }

    /// Discard the cached bearer so the next `bearer()` call logs in again.
    pub async fn invalidate(&self) {
        *self.bearer.write().await = None;
    }

    /// A solved captcha replaces any configured one for later logins.
    pub async fn set_solved_captcha(&self, token: String) {
        *self.credentials.solved_captcha.write().await = Some(token);
    }

    /// The CSRF value every state-changing request has to carry. The login
    /// response rotates the cookie, so this is read per request rather than
    /// cached at construction.
    pub async fn csrf_header(&self) -> Result<String, UploadError> {
        self.refresh_csrf().await
    }

    async fn login(&self) -> Result<Bearer, UploadError> {
        let captcha_token = self.captcha_token().await?;

        let xsrf = self.refresh_csrf().await?;

        let response = self
            .client
            .post(LOGIN_API_URL)
            .header(XSRF_HEADER, xsrf)
            .json(&serde_json::json!({
                "username": self.credentials.username,
                "password": self.credentials.password,
                "captcha_token": captcha_token,
            }))
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let (status_code, body) = parse_json(response).await?;
        if is_cloudflare_challenge(status_code, &body) {
            return Err(cloudflare_error());
        }
        if !(200..300).contains(&status_code) {
            return Err(crate::error_from_value(
                &serde_json::from_str::<serde_json::Value>(&body).unwrap_or_default(),
                status_code,
                "goonbox rejected the login",
            ));
        }

        let json: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        let login: LoginResponse = serde_json::from_value(json).map_err(|e| UploadError {
            message: format!("unreadable goonbox login response: {e}"),
            status_code: Some(status_code),
        })?;

        let token: Bearer = login
            .token
            .filter(|token| !token.trim().is_empty())
            .map(|token| Arc::from(token.as_str()))
            .ok_or_else(|| UploadError {
                message: "goonbox logged in but returned no bearer token".to_string(),
                status_code: Some(status_code),
            })?;

        *self.bearer.write().await = Some(token.clone());
        Ok(token)
    }

    /// Sanctum answers 419 unless the request echoes the `XSRF-TOKEN` cookie as
    /// a header, and priming that cookie is also what tells us the edge let us
    /// through: a blocked request returns 403 with the interstitial instead.
    async fn refresh_csrf(&self) -> Result<String, UploadError> {
        let response = self
            .client
            .get(CSRF_URL)
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let status_code = response.status().as_u16();
        if !response.status().is_success() {
            return Err(cloudflare_error_for(status_code));
        }

        let set_cookie = response
            .headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .find_map(|value| {
                let (name, rest) = value.split_once('=')?;
                (name.trim() == XSRF_COOKIE).then(|| rest.split(';').next().unwrap_or(""))
            })
            .filter(|value| !value.is_empty())
            .ok_or_else(|| UploadError {
                message: "goonbox set no CSRF cookie; Cloudflare is likely serving a challenge"
                    .to_string(),
                status_code: Some(status_code),
            })?;

        Ok(percent_decode(set_cookie))
    }

    async fn captcha_token(&self) -> Result<String, UploadError> {
        self.credentials
            .solved_captcha
            .read()
            .await
            .clone()
            .filter(|token| !token.trim().is_empty())
            .ok_or_else(|| UploadError {
                message: "no goonbox captcha token has been solved yet".to_string(),
                status_code: None,
            })
    }

    pub async fn refresh_captcha(&self) -> Result<String, UploadError> {
        crate::waf::solve(LOGIN_URL, &self.credentials.profile_dir)
            .await
            .map_err(|e| UploadError {
                message: format!("could not solve the goonbox captcha: {e:#}"),
                status_code: None,
            })
    }
}

pub fn map_reqwest_error(error: reqwest::Error) -> UploadError {
    UploadError {
        message: error.to_string(),
        status_code: error.status().map(|s| s.as_u16()),
    }
}

pub async fn parse_json(response: reqwest::Response) -> Result<(u16, String), UploadError> {
    let status_code = response.status().as_u16();
    let body = response.text().await.map_err(map_reqwest_error)?;
    Ok((status_code, body))
}

/// Cloudflare's interstitial arrives as a 403 whose HTML says "Just a moment...";
/// anything else is a real answer from Laravel and must not be reported as a
/// challenge, or the caller retries a request that already failed for a reason.
pub fn is_cloudflare_challenge(status_code: u16, body: &str) -> bool {
    status_code == 403 && body.to_lowercase().contains("just a moment")
}

pub fn cloudflare_error() -> UploadError {
    cloudflare_error_for(403)
}

pub fn cloudflare_error_for(status_code: u16) -> UploadError {
    UploadError {
        message: "Cloudflare blocked the request (browser headers missing or an interstitial \
                  challenge is being served)"
            .to_string(),
        status_code: Some(status_code),
    }
}

/// Cookies are stored percent-encoded but the header must carry the raw value.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }

    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        Session::new(Credentials {
            username: "user".into(),
            password: "pass".into(),
            solved_captcha: RwLock::new(None),
            profile_dir: std::env::temp_dir().join("upio-gb-test-profile"),
        })
        .expect("client should build")
    }

    #[test]
    fn browser_headers_cover_what_cloudflare_checks() {
        let headers = browser_headers();
        assert_eq!(headers[ACCEPT], "application/json, text/plain, */*");
        assert_eq!(headers.get("sec-fetch-dest").unwrap(), "empty");
        assert_eq!(headers.get("sec-fetch-mode").unwrap(), "cors");
        assert_eq!(headers.get("sec-fetch-site").unwrap(), "same-origin");
        assert!(headers.contains_key(USER_AGENT));
        assert_eq!(headers[ORIGIN], BASE_URL);
    }

    #[tokio::test]
    async fn login_without_a_solved_captcha_fails_before_the_request() {
        let err = session()
            .bearer()
            .await
            .expect_err("no captcha has been solved yet");
        assert!(err.message.contains("captcha"), "got: {}", err.message);
    }

    #[tokio::test]
    async fn a_blank_solved_captcha_is_rejected() {
        let s = session();
        s.set_solved_captcha("   ".to_string()).await;
        let err = s
            .bearer()
            .await
            .expect_err("a blank token should be rejected");
        assert!(err.message.contains("captcha"), "got: {}", err.message);
    }

    #[test]
    fn the_cloudflare_interstitial_is_told_apart_from_real_errors() {
        assert!(is_cloudflare_challenge(
            403,
            "<html><title>Just a moment...</title></html>"
        ));
        assert!(!is_cloudflare_challenge(
            403,
            "{\"message\":\"Unauthenticated\"}"
        ));
        assert!(!is_cloudflare_challenge(419, "Just a moment..."));
        assert!(!is_cloudflare_challenge(403, "{\"message\":\"not json\"}"));
    }

    #[test]
    fn the_client_targets_goonbox() {
        let request = session().client().get(CSRF_URL).build().unwrap();
        assert_eq!(request.url().as_str(), CSRF_URL);
    }
}
