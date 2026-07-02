use reqwest::blocking::Client as HttpClient;
use rusty_crew_core_protocol::{
    CoreError, CoreErrorKind, CoreResult, ModelProviderSecretEnvelope,
    MODEL_PROVIDER_SECRET_ENVELOPE_VERSION,
};
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;
use thiserror::Error;
use time::format_description::well_known::Rfc3339;
use time::{Duration as TimeDuration, OffsetDateTime};

const DEFAULT_TOKEN_ENDPOINT_PATH: &str = "/oauth/token";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiOauthCodeExchangeRequest {
    pub issuer: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub code: String,
    pub code_verifier: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiOauthTokenExchangeResult {
    pub id_token: String,
    pub access_token: String,
    pub refresh_token: String,
    pub exchanged_api_token: Option<String>,
    pub access_token_expires_at: Option<String>,
    pub email: Option<String>,
    pub account_id: Option<String>,
    pub plan_type: Option<String>,
    pub is_fedramp_account: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiOauthBearerResolution {
    pub bearer_token: String,
    pub account_id: Option<String>,
    pub is_fedramp_account: bool,
    pub refreshed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiOauthRefreshPolicy {
    pub refresh_before_expiry_seconds: i64,
    pub max_seconds_since_refresh: i64,
    pub exchange_api_token: bool,
}

impl Default for OpenAiOauthRefreshPolicy {
    fn default() -> Self {
        Self {
            refresh_before_expiry_seconds: 5 * 60,
            max_seconds_since_refresh: 8 * 24 * 60 * 60,
            exchange_api_token: true,
        }
    }
}

pub trait OpenAiOauthSecretStore {
    fn load_openai_oauth_secret(&mut self, provider_alias: &str) -> CoreResult<Option<String>>;
    fn save_openai_oauth_secret(
        &mut self,
        provider_alias: &str,
        secret_storage_text: String,
    ) -> CoreResult<()>;
}

#[derive(Debug, Error)]
pub enum OpenAiOauthError {
    #[error("OpenAI OAuth request transport failed")]
    Transport,
    #[error("OpenAI OAuth endpoint returned status {status}: {message}")]
    Status {
        status: u16,
        reason_code: Option<String>,
        message: String,
    },
    #[error("OpenAI OAuth endpoint returned malformed JSON: {0}")]
    MalformedResponse(String),
    #[error("OpenAI OAuth provider {provider_alias} has no stored credential")]
    MissingCredential { provider_alias: String },
    #[error("OpenAI OAuth provider {provider_alias} credential is not an OpenAI OAuth envelope")]
    WrongCredentialKind { provider_alias: String },
    #[error("OpenAI OAuth credential for {provider_alias} is invalid: {message}")]
    InvalidCredential {
        provider_alias: String,
        message: String,
    },
    #[error("OpenAI OAuth credential store failed: {0}")]
    Store(String),
}

impl From<OpenAiOauthError> for CoreError {
    fn from(error: OpenAiOauthError) -> Self {
        CoreError::new(CoreErrorKind::InvalidInput, error.to_string())
    }
}

pub struct OpenAiOauthClient {
    client: HttpClient,
}

impl OpenAiOauthClient {
    pub fn new() -> Result<Self, OpenAiOauthError> {
        let client = HttpClient::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| OpenAiOauthError::Transport)?;
        Ok(Self { client })
    }

    pub fn with_client(client: HttpClient) -> Self {
        Self { client }
    }

    pub fn exchange_authorization_code(
        &self,
        request: &OpenAiOauthCodeExchangeRequest,
    ) -> Result<OpenAiOauthTokenExchangeResult, OpenAiOauthError> {
        #[derive(Deserialize)]
        struct TokenResponse {
            id_token: String,
            access_token: String,
            refresh_token: String,
        }

        let response = self
            .client
            .post(token_endpoint(&request.issuer))
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", request.code.as_str()),
                ("redirect_uri", request.redirect_uri.as_str()),
                ("client_id", request.client_id.as_str()),
                ("code_verifier", request.code_verifier.as_str()),
            ])
            .send()
            .map_err(|_| OpenAiOauthError::Transport)?;

        let tokens: TokenResponse = decode_token_response(response)?;
        let exchanged_api_token =
            self.exchange_api_token(&request.issuer, &request.client_id, &tokens.id_token)?;
        let metadata = token_metadata(&tokens.id_token, &tokens.access_token);
        Ok(OpenAiOauthTokenExchangeResult {
            id_token: tokens.id_token,
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            exchanged_api_token: Some(exchanged_api_token),
            access_token_expires_at: metadata.access_token_expires_at,
            email: metadata.email,
            account_id: metadata.account_id,
            plan_type: metadata.plan_type,
            is_fedramp_account: metadata.is_fedramp_account,
        })
    }

    pub fn exchange_api_token(
        &self,
        issuer: &str,
        client_id: &str,
        id_token: &str,
    ) -> Result<String, OpenAiOauthError> {
        #[derive(Deserialize)]
        struct ExchangeResponse {
            access_token: String,
        }

        let response = self
            .client
            .post(token_endpoint(issuer))
            .form(&[
                (
                    "grant_type",
                    "urn:ietf:params:oauth:grant-type:token-exchange",
                ),
                ("client_id", client_id),
                ("requested_token", "openai-api-key"),
                ("subject_token", id_token),
                (
                    "subject_token_type",
                    "urn:ietf:params:oauth:token-type:id_token",
                ),
            ])
            .send()
            .map_err(|_| OpenAiOauthError::Transport)?;

        let body: ExchangeResponse = decode_token_response(response)?;
        Ok(body.access_token)
    }

    pub fn refresh_envelope(
        &self,
        envelope: &ModelProviderSecretEnvelope,
        now: OffsetDateTime,
        policy: &OpenAiOauthRefreshPolicy,
    ) -> Result<ModelProviderSecretEnvelope, OpenAiOauthError> {
        let ModelProviderSecretEnvelope::OpenAiOauth {
            version,
            issuer,
            client_id,
            id_token,
            access_token,
            refresh_token,
            exchanged_api_token,
            account_id,
            email,
            plan_type,
            is_fedramp_account,
            ..
        } = envelope
        else {
            return Err(OpenAiOauthError::WrongCredentialKind {
                provider_alias: "<unknown>".to_string(),
            });
        };

        #[derive(Deserialize)]
        struct RefreshResponse {
            #[serde(default)]
            id_token: Option<String>,
            #[serde(default)]
            access_token: Option<String>,
            #[serde(default)]
            refresh_token: Option<String>,
        }

        let response = self
            .client
            .post(token_endpoint(issuer))
            .json(&serde_json::json!({
                "client_id": client_id,
                "grant_type": "refresh_token",
                "refresh_token": refresh_token,
            }))
            .send()
            .map_err(|_| OpenAiOauthError::Transport)?;

        let refreshed: RefreshResponse = decode_token_response(response)?;
        let next_id_token = refreshed.id_token.unwrap_or_else(|| id_token.clone());
        let next_access_token = refreshed
            .access_token
            .unwrap_or_else(|| access_token.clone());
        let next_refresh_token = refreshed
            .refresh_token
            .unwrap_or_else(|| refresh_token.clone());
        let next_exchanged_api_token = if policy.exchange_api_token {
            Some(self.exchange_api_token(issuer, client_id, &next_id_token)?)
        } else {
            exchanged_api_token.clone()
        };
        let metadata = token_metadata(&next_id_token, &next_access_token);

        Ok(ModelProviderSecretEnvelope::OpenAiOauth {
            version: *version,
            issuer: issuer.clone(),
            client_id: client_id.clone(),
            id_token: next_id_token,
            access_token: next_access_token,
            refresh_token: next_refresh_token,
            exchanged_api_token: next_exchanged_api_token,
            last_refresh_at: Some(format_rfc3339(now)),
            account_id: metadata.account_id.or_else(|| account_id.clone()),
            email: metadata.email.or_else(|| email.clone()),
            plan_type: metadata.plan_type.or_else(|| plan_type.clone()),
            is_fedramp_account: metadata.is_fedramp_account || *is_fedramp_account,
            access_token_expires_at: metadata.access_token_expires_at,
        })
    }
}

pub fn resolve_openai_oauth_bearer<S: OpenAiOauthSecretStore>(
    provider_alias: &str,
    store: &mut S,
    client: &OpenAiOauthClient,
    now: OffsetDateTime,
    policy: &OpenAiOauthRefreshPolicy,
) -> Result<OpenAiOauthBearerResolution, OpenAiOauthError> {
    let raw = store
        .load_openai_oauth_secret(provider_alias)
        .map_err(|error| OpenAiOauthError::Store(error.to_string()))?
        .ok_or_else(|| OpenAiOauthError::MissingCredential {
            provider_alias: provider_alias.to_string(),
        })?;
    let envelope = ModelProviderSecretEnvelope::from_storage_text(&raw).map_err(|error| {
        OpenAiOauthError::InvalidCredential {
            provider_alias: provider_alias.to_string(),
            message: error.to_string(),
        }
    })?;
    let ModelProviderSecretEnvelope::OpenAiOauth { .. } = envelope else {
        return Err(OpenAiOauthError::WrongCredentialKind {
            provider_alias: provider_alias.to_string(),
        });
    };

    let refreshed = should_refresh_openai_oauth_envelope(&envelope, now, policy);
    let envelope = if refreshed {
        let refreshed_envelope = client.refresh_envelope(&envelope, now, policy)?;
        let storage_text = refreshed_envelope.to_storage_text().map_err(|error| {
            OpenAiOauthError::InvalidCredential {
                provider_alias: provider_alias.to_string(),
                message: error.to_string(),
            }
        })?;
        store
            .save_openai_oauth_secret(provider_alias, storage_text)
            .map_err(|error| OpenAiOauthError::Store(error.to_string()))?;
        refreshed_envelope
    } else {
        envelope
    };

    let ModelProviderSecretEnvelope::OpenAiOauth {
        access_token,
        account_id,
        is_fedramp_account,
        ..
    } = envelope
    else {
        unreachable!("credential kind checked above");
    };

    Ok(OpenAiOauthBearerResolution {
        bearer_token: access_token,
        account_id,
        is_fedramp_account,
        refreshed,
    })
}

pub fn should_refresh_openai_oauth_envelope(
    envelope: &ModelProviderSecretEnvelope,
    now: OffsetDateTime,
    policy: &OpenAiOauthRefreshPolicy,
) -> bool {
    let ModelProviderSecretEnvelope::OpenAiOauth {
        access_token,
        last_refresh_at,
        access_token_expires_at,
        ..
    } = envelope
    else {
        return false;
    };

    let expires_at = jwt_expiration(access_token)
        .or_else(|| access_token_expires_at.as_deref().and_then(parse_rfc3339));
    if let Some(expires_at) = expires_at {
        if expires_at - now <= TimeDuration::seconds(policy.refresh_before_expiry_seconds) {
            return true;
        }
    }

    if let Some(last_refresh_at) = last_refresh_at.as_deref().and_then(parse_rfc3339) {
        if now - last_refresh_at >= TimeDuration::seconds(policy.max_seconds_since_refresh) {
            return true;
        }
    }

    false
}

fn decode_token_response<T: for<'de> Deserialize<'de>>(
    response: reqwest::blocking::Response,
) -> Result<T, OpenAiOauthError> {
    let status = response.status();
    let text = response.text().map_err(|_| {
        OpenAiOauthError::MalformedResponse("response body unavailable".to_string())
    })?;
    if !status.is_success() {
        let detail = token_endpoint_error_detail(&text);
        return Err(OpenAiOauthError::Status {
            status: status.as_u16(),
            reason_code: detail.reason_code,
            message: detail.message,
        });
    }
    serde_json::from_str(&text)
        .map_err(|error| OpenAiOauthError::MalformedResponse(error.to_string()))
}

fn token_endpoint(issuer: &str) -> String {
    format!(
        "{}{}",
        issuer.trim_end_matches('/'),
        DEFAULT_TOKEN_ENDPOINT_PATH
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TokenEndpointErrorDetail {
    reason_code: Option<String>,
    message: String,
}

fn token_endpoint_error_detail(text: &str) -> TokenEndpointErrorDetail {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return TokenEndpointErrorDetail {
            reason_code: None,
            message: "non-json OAuth error body redacted".to_string(),
        };
    };
    if let Some(message) = value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
    {
        let reason_code = value
            .get("error")
            .and_then(|error| error.get("code"))
            .and_then(Value::as_str)
            .map(str::to_string);
        return TokenEndpointErrorDetail {
            reason_code,
            message: message.to_string(),
        };
    }
    if let Some(message) = value.get("error_description").and_then(Value::as_str) {
        return TokenEndpointErrorDetail {
            reason_code: value
                .get("error")
                .and_then(Value::as_str)
                .map(str::to_string),
            message: message.to_string(),
        };
    }
    if let Some(code) = value.get("error").and_then(Value::as_str) {
        return TokenEndpointErrorDetail {
            reason_code: Some(code.to_string()),
            message: code.to_string(),
        };
    }
    TokenEndpointErrorDetail {
        reason_code: None,
        message: "OAuth endpoint returned an error".to_string(),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct TokenMetadata {
    access_token_expires_at: Option<String>,
    email: Option<String>,
    account_id: Option<String>,
    plan_type: Option<String>,
    is_fedramp_account: bool,
}

fn token_metadata(id_token: &str, access_token: &str) -> TokenMetadata {
    let mut metadata = TokenMetadata {
        access_token_expires_at: jwt_expiration(access_token).map(format_rfc3339),
        ..TokenMetadata::default()
    };
    if let Some(payload) = jwt_payload_json(id_token) {
        metadata.email = payload
            .get("email")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                payload
                    .get("https://api.openai.com/profile")
                    .and_then(|profile| profile.get("email"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            });
        if let Some(auth) = payload.get("https://api.openai.com/auth") {
            metadata.account_id = auth
                .get("chatgpt_account_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            metadata.plan_type = auth
                .get("chatgpt_plan_type")
                .and_then(Value::as_str)
                .map(str::to_string);
            metadata.is_fedramp_account = auth
                .get("chatgpt_account_is_fedramp")
                .and_then(Value::as_bool)
                .unwrap_or(false);
        }
    }
    metadata
}

fn jwt_expiration(jwt: &str) -> Option<OffsetDateTime> {
    let payload = jwt_payload_json(jwt)?;
    let exp = payload.get("exp")?.as_i64()?;
    OffsetDateTime::from_unix_timestamp(exp).ok()
}

fn jwt_payload_json(jwt: &str) -> Option<Value> {
    let payload = jwt.split('.').nth(1)?;
    let bytes = decode_base64_url_no_pad(payload).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn decode_base64_url_no_pad(input: &str) -> Result<Vec<u8>, ()> {
    let mut buffer: u32 = 0;
    let mut bits: u8 = 0;
    let mut output = Vec::with_capacity(input.len() * 3 / 4);
    for byte in input.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            b'=' => break,
            _ => return Err(()),
        } as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        while bits >= 8 {
            bits -= 8;
            output.push(((buffer >> bits) & 0xff) as u8);
        }
        if bits > 0 {
            buffer &= (1 << bits) - 1;
        } else {
            buffer = 0;
        }
    }
    Ok(output)
}

fn parse_rfc3339(value: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339).ok()
}

fn format_rfc3339(value: OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_else(|_| value.to_string())
}

pub fn openai_oauth_envelope_from_exchange_result(
    result: OpenAiOauthTokenExchangeResult,
    issuer: String,
    client_id: String,
    now: OffsetDateTime,
) -> ModelProviderSecretEnvelope {
    ModelProviderSecretEnvelope::OpenAiOauth {
        version: MODEL_PROVIDER_SECRET_ENVELOPE_VERSION,
        issuer,
        client_id,
        id_token: result.id_token,
        access_token: result.access_token,
        refresh_token: result.refresh_token,
        exchanged_api_token: result.exchanged_api_token,
        last_refresh_at: Some(format_rfc3339(now)),
        account_id: result.account_id,
        email: result.email,
        plan_type: result.plan_type,
        is_fedramp_account: result.is_fedramp_account,
        access_token_expires_at: result.access_token_expires_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, VecDeque};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;

    #[test]
    fn exchanges_authorization_code_and_api_token() {
        let server = FakeOauthServer::new(vec![
            fake_json(
                200,
                r#"{"id_token":"id.jwt.sig","access_token":"access.jwt.sig","refresh_token":"refresh.jwt"}"#,
            ),
            fake_json(200, r#"{"access_token":"exchanged-api-token"}"#),
        ]);
        let client = OpenAiOauthClient::new().unwrap();

        let result = client
            .exchange_authorization_code(&OpenAiOauthCodeExchangeRequest {
                issuer: server.issuer(),
                client_id: "client-1".to_string(),
                redirect_uri: "http://localhost/callback".to_string(),
                code: "authorization-code-secret".to_string(),
                code_verifier: "pkce-verifier-secret".to_string(),
            })
            .unwrap();

        assert_eq!(result.refresh_token, "refresh.jwt");
        assert_eq!(
            result.exchanged_api_token.as_deref(),
            Some("exchanged-api-token")
        );
        let requests = server.requests();
        assert!(requests[0].body.contains("grant_type=authorization_code"));
        assert!(requests[0].body.contains("code=authorization-code-secret"));
        assert!(requests[0]
            .body
            .contains("code_verifier=pkce-verifier-secret"));
        assert!(requests[1].body.contains("requested_token=openai-api-key"));
        assert!(requests[1].body.contains("subject_token=id.jwt.sig"));
    }

    #[test]
    fn non_success_status_uses_redacted_error_message() {
        let server = FakeOauthServer::new(vec![fake_json(
            400,
            r#"{"error":{"message":"Your refresh token has already been used.","code":"refresh_token_reused"}}"#,
        )]);
        let client = OpenAiOauthClient::new().unwrap();
        let error = client
            .exchange_api_token(&server.issuer(), "client-secret", "id-token-secret")
            .unwrap_err();

        assert!(matches!(
            &error,
            OpenAiOauthError::Status {
                reason_code: Some(reason_code),
                ..
            } if reason_code == "refresh_token_reused"
        ));
        let rendered = error.to_string();
        assert!(rendered.contains("status 400"));
        assert!(rendered.contains("already been used"));
        assert!(!rendered.contains("id-token-secret"));
        assert!(!rendered.contains("client-secret"));
    }

    #[test]
    fn malformed_success_response_is_reported_without_tokens() {
        let server = FakeOauthServer::new(vec![fake_json(200, r#"{"access_token":42}"#)]);
        let client = OpenAiOauthClient::new().unwrap();
        let error = client
            .exchange_api_token(&server.issuer(), "client-secret", "id-token-secret")
            .unwrap_err();

        let rendered = error.to_string();
        assert!(rendered.contains("malformed JSON"));
        assert!(!rendered.contains("client-secret"));
        assert!(!rendered.contains("id-token-secret"));
    }

    #[test]
    fn non_json_error_body_is_redacted() {
        let server = FakeOauthServer::new(vec![fake_json(
            500,
            "backend echoed code=secret-code and verifier=secret-verifier",
        )]);
        let client = OpenAiOauthClient::new().unwrap();
        let error = client
            .exchange_api_token(&server.issuer(), "client-secret", "id-token-secret")
            .unwrap_err();

        let rendered = error.to_string();
        assert!(rendered.contains("non-json OAuth error body redacted"));
        assert!(!rendered.contains("secret-code"));
        assert!(!rendered.contains("secret-verifier"));
    }

    #[test]
    fn expired_oauth_secret_refreshes_and_persists_update() {
        let now = OffsetDateTime::from_unix_timestamp(2_000).unwrap();
        let server = FakeOauthServer::new(vec![
            fake_json(
                200,
                &format!(
                    r#"{{"id_token":"{}","access_token":"{}","refresh_token":"fresh-refresh"}}"#,
                    test_jwt(2_700, serde_json::json!({"email": "fresh@example.test"})),
                    test_jwt(2_600, serde_json::json!({"scope": "responses"}))
                ),
            ),
            fake_json(200, r#"{"access_token":"fresh-exchanged-api-token"}"#),
        ]);
        let mut store = MemorySecretStore::new();
        let stale = ModelProviderSecretEnvelope::OpenAiOauth {
            version: MODEL_PROVIDER_SECRET_ENVELOPE_VERSION,
            issuer: server.issuer(),
            client_id: "client-1".to_string(),
            id_token: test_jwt(2_500, serde_json::json!({"email": "old@example.test"})),
            access_token: test_jwt(2_050, serde_json::json!({})),
            refresh_token: "old-refresh".to_string(),
            exchanged_api_token: Some("old-exchanged".to_string()),
            last_refresh_at: Some(format_rfc3339(
                OffsetDateTime::from_unix_timestamp(1_000).unwrap(),
            )),
            account_id: None,
            email: Some("old@example.test".to_string()),
            plan_type: None,
            is_fedramp_account: false,
            access_token_expires_at: None,
        };
        store.insert("gpt", stale.to_storage_text().unwrap());

        let resolution = resolve_openai_oauth_bearer(
            "gpt",
            &mut store,
            &OpenAiOauthClient::new().unwrap(),
            now,
            &OpenAiOauthRefreshPolicy::default(),
        )
        .unwrap();

        assert!(resolution.refreshed);
        assert_eq!(
            resolution.bearer_token,
            test_jwt(2_600, serde_json::json!({"scope": "responses"}))
        );
        let saved =
            ModelProviderSecretEnvelope::from_storage_text(store.get("gpt").unwrap()).unwrap();
        let ModelProviderSecretEnvelope::OpenAiOauth {
            refresh_token,
            exchanged_api_token,
            last_refresh_at,
            email,
            ..
        } = saved
        else {
            panic!("expected openai oauth envelope");
        };
        assert_eq!(refresh_token, "fresh-refresh");
        assert_eq!(
            exchanged_api_token.as_deref(),
            Some("fresh-exchanged-api-token")
        );
        assert_eq!(last_refresh_at.as_deref(), Some("1970-01-01T00:33:20Z"));
        assert_eq!(email.as_deref(), Some("fresh@example.test"));
    }

    #[test]
    fn recent_oauth_secret_does_not_refresh() {
        let now = OffsetDateTime::from_unix_timestamp(2_000).unwrap();
        let mut store = MemorySecretStore::new();
        let envelope = ModelProviderSecretEnvelope::OpenAiOauth {
            version: MODEL_PROVIDER_SECRET_ENVELOPE_VERSION,
            issuer: "http://127.0.0.1:9".to_string(),
            client_id: "client-1".to_string(),
            id_token: test_jwt(5_000, serde_json::json!({})),
            access_token: test_jwt(5_000, serde_json::json!({})),
            refresh_token: "refresh".to_string(),
            exchanged_api_token: None,
            last_refresh_at: Some(format_rfc3339(now)),
            account_id: Some("account".to_string()),
            email: None,
            plan_type: None,
            is_fedramp_account: true,
            access_token_expires_at: None,
        };
        let original = envelope.to_storage_text().unwrap();
        store.insert("gpt", original.clone());

        let resolution = resolve_openai_oauth_bearer(
            "gpt",
            &mut store,
            &OpenAiOauthClient::new().unwrap(),
            now,
            &OpenAiOauthRefreshPolicy::default(),
        )
        .unwrap();

        assert!(!resolution.refreshed);
        assert_eq!(resolution.account_id.as_deref(), Some("account"));
        assert_eq!(store.get("gpt"), Some(original.as_str()));
    }

    #[test]
    fn missing_oauth_secret_fails_before_transport() {
        let mut store = MemorySecretStore::new();
        let error = resolve_openai_oauth_bearer(
            "missing",
            &mut store,
            &OpenAiOauthClient::new().unwrap(),
            OffsetDateTime::from_unix_timestamp(2_000).unwrap(),
            &OpenAiOauthRefreshPolicy::default(),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            OpenAiOauthError::MissingCredential { provider_alias } if provider_alias == "missing"
        ));
    }

    #[derive(Debug, Clone)]
    struct FakeResponse {
        status: u16,
        body: String,
    }

    #[derive(Debug, Clone, Default)]
    struct CapturedRequest {
        body: String,
    }

    struct FakeOauthServer {
        addr: String,
        requests: Arc<Mutex<Vec<CapturedRequest>>>,
    }

    impl FakeOauthServer {
        fn new(responses: Vec<FakeResponse>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap().to_string();
            let responses = Arc::new(Mutex::new(VecDeque::from(responses)));
            let requests = Arc::new(Mutex::new(Vec::new()));
            let requests_for_thread = Arc::clone(&requests);
            thread::spawn(move || loop {
                let Some(response) = responses.lock().unwrap().pop_front() else {
                    break;
                };
                let (mut stream, _) = listener.accept().unwrap();
                let mut buffer = Vec::new();
                let mut chunk = [0_u8; 4096];
                loop {
                    let read = stream.read(&mut chunk).unwrap();
                    if read == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..read]);
                    if request_complete(&buffer) {
                        break;
                    }
                }
                let body = String::from_utf8_lossy(&buffer)
                    .split_once("\r\n\r\n")
                    .map(|(_, body)| body.to_string())
                    .unwrap_or_default();
                requests_for_thread
                    .lock()
                    .unwrap()
                    .push(CapturedRequest { body });
                let response_text = format!(
                        "HTTP/1.1 {} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        response.status,
                        response.body.len(),
                        response.body
                    );
                stream.write_all(response_text.as_bytes()).unwrap();
            });
            Self { addr, requests }
        }

        fn issuer(&self) -> String {
            format!("http://{}", self.addr)
        }

        fn requests(&self) -> Vec<CapturedRequest> {
            self.requests.lock().unwrap().clone()
        }
    }

    fn request_complete(buffer: &[u8]) -> bool {
        let text = String::from_utf8_lossy(buffer);
        let Some((headers, body)) = text.split_once("\r\n\r\n") else {
            return false;
        };
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.strip_prefix("content-length:")
                    .or_else(|| line.strip_prefix("Content-Length:"))
            })
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        body.len() >= content_length
    }

    fn fake_json(status: u16, body: &str) -> FakeResponse {
        FakeResponse {
            status,
            body: body.to_string(),
        }
    }

    #[derive(Default)]
    struct MemorySecretStore {
        values: HashMap<String, String>,
    }

    impl MemorySecretStore {
        fn new() -> Self {
            Self::default()
        }

        fn insert(&mut self, alias: &str, value: String) {
            self.values.insert(alias.to_string(), value);
        }

        fn get(&self, alias: &str) -> Option<&str> {
            self.values.get(alias).map(String::as_str)
        }
    }

    impl OpenAiOauthSecretStore for MemorySecretStore {
        fn load_openai_oauth_secret(&mut self, provider_alias: &str) -> CoreResult<Option<String>> {
            Ok(self.values.get(provider_alias).cloned())
        }

        fn save_openai_oauth_secret(
            &mut self,
            provider_alias: &str,
            secret_storage_text: String,
        ) -> CoreResult<()> {
            self.values
                .insert(provider_alias.to_string(), secret_storage_text);
            Ok(())
        }
    }

    fn test_jwt(exp: i64, extra: Value) -> String {
        let mut payload = serde_json::json!({"exp": exp});
        let Value::Object(payload_map) = &mut payload else {
            unreachable!();
        };
        if let Value::Object(extra_map) = extra {
            for (key, value) in extra_map {
                payload_map.insert(key, value);
            }
        }
        format!(
            "{}.{}.{}",
            base64_url(r#"{"alg":"none"}"#.as_bytes()),
            base64_url(serde_json::to_string(&payload).unwrap().as_bytes()),
            "sig"
        )
    }

    fn base64_url(bytes: &[u8]) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut output = String::new();
        let mut index = 0;
        while index < bytes.len() {
            let a = bytes[index];
            let b = bytes.get(index + 1).copied().unwrap_or(0);
            let c = bytes.get(index + 2).copied().unwrap_or(0);
            output.push(TABLE[(a >> 2) as usize] as char);
            output.push(TABLE[(((a & 0b0000_0011) << 4) | (b >> 4)) as usize] as char);
            if index + 1 < bytes.len() {
                output.push(TABLE[(((b & 0b0000_1111) << 2) | (c >> 6)) as usize] as char);
            }
            if index + 2 < bytes.len() {
                output.push(TABLE[(c & 0b0011_1111) as usize] as char);
            }
            index += 3;
        }
        output
    }
}
