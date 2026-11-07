//! Delegated Microsoft personal-account SMTP. Secrets exist only in memory and
//! the native credential store; config contains a public client ID and reference.
use crate::{
    config::{EmailConfig, SmtpAuthentication, SmtpSecurity},
    smtp_credentials::Store,
};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zeroize::{Zeroize, Zeroizing};

const SCOPE: &str = "https://outlook.office.com/SMTP.Send offline_access";
#[cfg(not(test))]
const AUTHORITY: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0";
const RECONNECT: &str = "Microsoft authorization is missing, expired or revoked. Sign in / reconnect in Email Settings.";
const VAULT: &str = "Microsoft credential store unavailable or verification failed. Unlock your OS credential store and reconnect; no fallback is permitted.";
const INVALID: &str =
    "Microsoft returned an invalid authorization response; no credentials were accepted.";
const NETWORK: &str = "Microsoft authorization connection failed. Check your network, VPN and HTTPS proxy settings; request details are withheld.";
// A refresh token is replaced only after a verified native-store write. This
// mutex also prevents an in-process send and sign-in racing token refresh.
static TOKENS: Mutex<()> = Mutex::new(());

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Context {
    client_id: String,
    username: String,
}
impl Context {
    fn from_email(email: &EmailConfig) -> Result<Self, String> {
        if email.smtp_transport != "SMTP Server"
            || email.smtp_authentication != SmtpAuthentication::MicrosoftOAuth2
        {
            return Err("Select Microsoft OAuth2 authentication for remote SMTP.".into());
        }
        if !email
            .smtp_host
            .eq_ignore_ascii_case("smtp-mail.outlook.com")
            || email.smtp_port != 587
            || email.smtp_security != Some(SmtpSecurity::RequiredStarttls)
        {
            return Err("Microsoft personal SMTP requires smtp-mail.outlook.com:587 with required STARTTLS.".into());
        }
        let username = email.smtp_username.trim();
        if !username.is_ascii()
            || username
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
            || username.matches('@').count() != 1
            || username.starts_with('@')
            || username.ends_with('@')
        {
            return Err("Enter the full Microsoft mailbox address as the SMTP username.".into());
        }
        let client = uuid::Uuid::parse_str(email.microsoft_client_id.trim())
            .map_err(|_| "Enter the registered Microsoft public-client application ID (UUID).")?;
        if client.is_nil() {
            return Err("A real Microsoft application registration is required.".into());
        }
        Ok(Self {
            client_id: client.to_string(),
            username: username.to_ascii_lowercase(),
        })
    }
}

pub(crate) fn requires_oauth(email: &EmailConfig) -> bool {
    if email.smtp_transport != "SMTP Server" {
        return false;
    }
    let domain = email
        .smtp_username
        .trim()
        .rsplit_once('@')
        .map(|(_, d)| d.to_ascii_lowercase());
    matches!(
        domain.as_deref(),
        Some("outlook.com" | "hotmail.com" | "live.com" | "msn.com")
    ) || [
        "smtp-mail.outlook.com",
        "smtp.office365.com",
        "smtp.live.com",
    ]
    .iter()
    .any(|h| email.smtp_host.trim().eq_ignore_ascii_case(h))
}

/// Pure readiness check, shared by dispatch and standalone test-email setup.
pub(crate) fn validate(email: &EmailConfig) -> Result<(), String> {
    if email.smtp_transport != "SMTP Server" {
        return Ok(());
    }
    if email.smtp_authentication == SmtpAuthentication::MicrosoftOAuth2 {
        Context::from_email(email)?;
        if !email
            .microsoft_credential_ref
            .as_deref()
            .is_some_and(valid_ref)
        {
            return Err(RECONNECT.into());
        }
    } else if requires_oauth(email) {
        return Err("Microsoft SMTP requires OAuth2. Password/app-password authentication is blocked; select Microsoft OAuth2 and sign in.".into());
    }
    Ok(())
}
fn valid_ref(id: &str) -> bool {
    id.strip_prefix("oauth-v1-")
        .is_some_and(|v| v.len() == 32 && v.bytes().all(|b| b.is_ascii_hexdigit()))
}
fn now() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_secs())
        .map_err(|_| "System clock is invalid; correct it before Microsoft sign-in.".into())
}
fn secret_valid(value: &str) -> bool {
    !value.is_empty() && value.len() <= 32768 && value.bytes().all(|b| b.is_ascii_graphic())
}
#[derive(Serialize, Deserialize)]
struct Tokens {
    context: Context,
    access: String,
    refresh: String,
    expires_at: u64,
}
impl Drop for Tokens {
    fn drop(&mut self) {
        self.access.zeroize();
        self.refresh.zeroize();
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Manifest {
    generation: String,
    parts: usize,
}
const PART_SIZE: usize = 1000; // Below Windows Credential Manager limits, even as UTF-16.
fn read(store: &dyn Store, id: &str, context: &Context) -> Result<Tokens, String> {
    if !valid_ref(id) {
        return Err(RECONNECT.into());
    }
    if store.get(&revoked_key(id)).map_err(|_| VAULT)?.is_some() {
        return Err(RECONNECT.into());
    }
    let manifest: Manifest =
        serde_json::from_str(&store.get(id).map_err(|_| VAULT)?.ok_or(RECONNECT)?)
            .map_err(|_| VAULT)?;
    if manifest.generation.len() != 32
        || !manifest.generation.bytes().all(|b| b.is_ascii_hexdigit())
        || manifest.parts == 0
        || manifest.parts > 128
    {
        return Err(VAULT.into());
    }
    let mut raw = Zeroizing::new(String::new());
    for part in 0..manifest.parts {
        let value = Zeroizing::new(
            store
                .get(&format!("oauth-part-v1-{}-{part}", manifest.generation))
                .map_err(|_| VAULT)?
                .ok_or(VAULT)?,
        );
        if value.is_empty() || value.len() > PART_SIZE {
            return Err(VAULT.into());
        }
        raw.push_str(&value);
    }
    let tokens: Tokens = serde_json::from_str(&raw).map_err(|_| VAULT)?;
    if &tokens.context != context || !secret_valid(&tokens.access) || !secret_valid(&tokens.refresh)
    {
        return Err(RECONNECT.into());
    }
    Ok(tokens)
}
fn write(store: &dyn Store, id: &str, tokens: &Tokens) -> Result<(), String> {
    if store.get(&revoked_key(id)).map_err(|_| VAULT)?.is_some() {
        return Err(RECONNECT.into());
    }
    let old = store
        .get(id)
        .map_err(|_| VAULT)?
        .map(|v| serde_json::from_str::<Manifest>(&v).map_err(|_| VAULT))
        .transpose()?;
    if let Some(old) = &old {
        validate_manifest(old)?;
        let key = generation_key(&old.generation);
        if store.get(&key).map_err(|_| VAULT)?.is_none() {
            put_verified(
                store,
                &key,
                &serde_json::to_string(&Generation {
                    manifest: old.clone(),
                    parent: None,
                })
                .map_err(|_| VAULT)?,
            )?;
        }
    }
    let raw = Zeroizing::new(serde_json::to_string(tokens).map_err(|_| VAULT)?);
    // Immutable chunks are fully read-verified BEFORE publishing the small
    // manifest. Retain previous generations for recovery; never delete secrets
    // behind an older config or concurrent reader.
    if !raw.is_ascii() || raw.len() > PART_SIZE * 128 {
        return Err(VAULT.into());
    }
    let manifest = Manifest {
        generation: uuid::Uuid::new_v4().simple().to_string(),
        parts: raw.len().div_ceil(PART_SIZE),
    };
    for (part, bytes) in raw.as_bytes().chunks(PART_SIZE).enumerate() {
        let key = format!("oauth-part-v1-{}-{part}", manifest.generation);
        if store.get(&key).map_err(|_| VAULT)?.is_some() {
            return Err(VAULT.into());
        }
        let value = std::str::from_utf8(bytes).map_err(|_| VAULT)?;
        store.put(&key, value).map_err(|_| VAULT)?;
        let check = Zeroizing::new(store.get(&key).map_err(|_| VAULT)?.ok_or(VAULT)?);
        if check.as_str() != value {
            return Err(VAULT.into());
        }
    }
    put_verified(
        store,
        &generation_key(&manifest.generation),
        &serde_json::to_string(&Generation {
            manifest: manifest.clone(),
            parent: old.map(|m| m.generation),
        })
        .map_err(|_| VAULT)?,
    )?;
    if store.get(&revoked_key(id)).map_err(|_| VAULT)?.is_some() {
        return Err(RECONNECT.into());
    }
    store
        .put(id, &serde_json::to_string(&manifest).map_err(|_| VAULT)?)
        .map_err(|_| VAULT)?;
    let check = read(store, id, &tokens.context)?;
    if check.access != tokens.access
        || check.refresh != tokens.refresh
        || check.expires_at != tokens.expires_at
    {
        return Err(VAULT.into());
    }
    Ok(())
}

// An explicit cleanup tombstone blocks honest clients and restored references.
// Microsoft consent/token revocation remains a separate user action.
fn revoked_key(id: &str) -> String {
    format!(
        "oauth-revoked-{}",
        id.strip_prefix("oauth-v1-").unwrap_or("invalid")
    )
}
fn generation_key(generation: &str) -> String {
    format!("oauth-generation-v1-{generation}")
}
#[derive(Serialize, Deserialize)]
struct Generation {
    manifest: Manifest,
    parent: Option<String>,
}
fn validate_manifest(m: &Manifest) -> Result<(), String> {
    if m.generation.len() != 32
        || !m.generation.bytes().all(|b| b.is_ascii_hexdigit())
        || m.parts == 0
        || m.parts > 128
    {
        return Err(VAULT.into());
    }
    Ok(())
}
fn put_verified(store: &dyn Store, key: &str, value: &str) -> Result<(), String> {
    store.put(key, value).map_err(|_| VAULT)?;
    if store.get(key).map_err(|_| VAULT)?.as_deref() != Some(value) {
        return Err(VAULT.into());
    }
    Ok(())
}
fn cleanup_with(store: &dyn Store, email: &EmailConfig) -> Result<(), String> {
    let id = email.microsoft_credential_ref.as_deref().ok_or(RECONNECT)?;
    if !valid_ref(id) {
        return Err(RECONNECT.into());
    }
    let tombstone = revoked_key(id);
    let root = match store.get(&tombstone).map_err(|_| VAULT)? {
        Some(v) => serde_json::from_str::<Manifest>(&v).map_err(|_| VAULT)?,
        None => {
            read(store, id, &Context::from_email(email)?)?;
            let v = store.get(id).map_err(|_| VAULT)?.ok_or(VAULT)?;
            let manifest = serde_json::from_str::<Manifest>(&v).map_err(|_| VAULT)?;
            validate_manifest(&manifest)?;
            put_verified(store, &tombstone, &v)?;
            manifest
        }
    };
    // Build and validate the complete known chain before deleting any chunks.
    let mut manifests = Vec::new();
    let mut next = Some(root);
    let mut seen = std::collections::HashSet::new();
    while let Some(m) = next.take() {
        validate_manifest(&m)?;
        if !seen.insert(m.generation.clone()) || manifests.len() >= 10000 {
            return Err(VAULT.into());
        }
        let meta = store
            .get(&generation_key(&m.generation))
            .map_err(|_| VAULT)?;
        next = if let Some(raw) = meta {
            let g: Generation = serde_json::from_str(&raw).map_err(|_| VAULT)?;
            if g.manifest.generation != m.generation || g.manifest.parts != m.parts {
                return Err(VAULT.into());
            }
            if let Some(parent) = g.parent {
                let raw = store
                    .get(&generation_key(&parent))
                    .map_err(|_| VAULT)?
                    .ok_or(VAULT)?;
                Some(
                    serde_json::from_str::<Generation>(&raw)
                        .map_err(|_| VAULT)?
                        .manifest,
                )
            } else {
                None
            }
        } else {
            None
        };
        manifests.push(m);
    }
    store.delete(id).map_err(|_| VAULT)?;
    for m in &manifests {
        for part in 0..m.parts {
            let key = format!("oauth-part-v1-{}-{part}", m.generation);
            store.delete(&key).map_err(|_| VAULT)?;
            if store.get(&key).map_err(|_| VAULT)?.is_some() {
                return Err(VAULT.into());
            }
        }
    }
    // Keep non-secret chain metadata/tombstone for idempotent retries and backups.
    if store.get(id).map_err(|_| VAULT)?.is_some() {
        return Err(VAULT.into());
    }
    Ok(())
}
pub(crate) fn forget_local(email: &EmailConfig) -> Result<(), String> {
    let _guard = TOKENS.lock().map_err(|_| VAULT)?;
    cleanup_with(crate::smtp_credentials::store(), email)
}

// Transport is injected in tests; production endpoints are fixed HTTPS, with
// certificate verification, no redirects, bounded responses and a deadline.
trait Http {
    fn post(&self, endpoint: &str, form: &[(&str, &str)]) -> Result<Zeroizing<String>, String>;
}
struct MicrosoftHttp;

// ureq 3.4's TCP connector stops on an unreachable route instead of trying
// the remaining DNS addresses. Retry only connection establishment, before
// any OAuth HTTP body is sent, preserving the default TLS/proxy chain.
#[derive(Debug)]
struct AddressFallback<C>(C);
impl<C: ureq::unversioned::transport::Connector> ureq::unversioned::transport::Connector
    for AddressFallback<C>
{
    type Out = C::Out;
    fn connect(
        &self,
        details: &ureq::unversioned::transport::ConnectionDetails,
        _: Option<()>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        use ureq::unversioned::transport::{time, ConnectionDetails};
        let start = Instant::now();
        let mut last = ureq::Error::HostNotFound;
        for (index, address) in details.addrs.iter().enumerate() {
            let remaining = details.timeout.after.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                return Err(ureq::Error::Timeout(details.timeout.reason));
            }
            let mut timeout = details.timeout;
            timeout.after = time::Duration::Exact(remaining / (details.addrs.len() - index) as u32);
            let mut addrs = details.resolver.empty();
            addrs.push(*address);
            let attempt = ConnectionDetails {
                uri: details.uri,
                addrs,
                config: details.config,
                request_level: details.request_level,
                resolver: details.resolver,
                now: (details.current_time)(),
                timeout,
                current_time: details.current_time.clone(),
                run_connector: details.run_connector.clone(),
            };
            match self.0.connect(&attempt, None) {
                Err(error) if retry_connect(&error) => last = error,
                result => return result,
            }
        }
        Err(last)
    }
}
fn retry_connect(error: &ureq::Error) -> bool {
    matches!(error, ureq::Error::Io(e) if matches!(e.kind(),
        std::io::ErrorKind::HostUnreachable | std::io::ErrorKind::NetworkUnreachable |
        std::io::ErrorKind::ConnectionRefused))
        || matches!(error, ureq::Error::Timeout(_))
}
#[cfg(not(test))]
fn microsoft_agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(0)
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(30)))
        .build();
    ureq::Agent::with_parts(
        config,
        AddressFallback(ureq::unversioned::transport::DefaultConnector::default()),
        ureq::unversioned::resolver::DefaultResolver::default(),
    )
}
// Do not format provider/library errors: they can contain proxy credentials,
// URLs, response bodies or certificate details. Only classify typed errors.
fn transport_error(error: &ureq::Error) -> &'static str {
    use std::io::ErrorKind;
    match error {
        ureq::Error::HostNotFound => "Microsoft sign-in: DNS lookup failed. Check your internet connection, DNS and VPN.",
        ureq::Error::Io(e) => match e.kind() {
            ErrorKind::HostUnreachable | ErrorKind::NetworkUnreachable => "Microsoft sign-in: no reachable network route. Check IPv4/IPv6 connectivity, VPN and firewall settings.",
            ErrorKind::ConnectionRefused => "Microsoft sign-in: connection refused. Check your firewall and HTTPS proxy.",
            ErrorKind::TimedOut => "Microsoft sign-in: connection timed out. Check your connection, VPN and HTTPS proxy.",
            _ => "Microsoft sign-in: network I/O failed. Check your connection, VPN and HTTPS proxy; request details are withheld.",
        },
        ureq::Error::Timeout(_) => "Microsoft sign-in: connection timed out. Check your connection, VPN and HTTPS proxy.",
        ureq::Error::Rustls(_) | ureq::Error::Tls(_) | ureq::Error::TlsRequired => "Microsoft sign-in: secure TLS connection failed. Check the system clock, certificate trust and HTTPS-inspecting proxy. Certificate verification remains required.",
        ureq::Error::InvalidProxyUrl | ureq::Error::ConnectProxyFailed(_) => "Microsoft sign-in: HTTPS proxy configuration or CONNECT failed. Check the application's proxy environment; credentials are withheld.",
        ureq::Error::TooManyRedirects | ureq::Error::RedirectFailed => "Microsoft sign-in: unexpected redirect blocked. Check for a captive portal or intercepting proxy.",
        ureq::Error::BodyExceedsLimit(_) => "Microsoft sign-in: response exceeded the safe size limit. Check for an intercepting proxy; response content is withheld.",
        _ => NETWORK,
    }
}
fn http_status_error(status: u16) -> String {
    match status {
        300..=399 => "Microsoft sign-in: unexpected HTTP redirect blocked. Check for a captive portal or proxy.".into(),
        407 => "Microsoft sign-in: proxy authentication required (HTTP 407). Check your HTTPS proxy configuration.".into(),
        429 => "Microsoft sign-in: service rate limit reached (HTTP 429). Wait before retrying.".into(),
        500..=599 => format!("Microsoft sign-in: service or gateway unavailable (HTTP {status}). Try again later; response content is withheld."),
        _ => format!("Microsoft sign-in: unexpected response (HTTP {status}). Check application registration or network policy; response content is withheld."),
    }
}
impl Http for MicrosoftHttp {
    fn post(&self, endpoint: &str, form: &[(&str, &str)]) -> Result<Zeroizing<String>, String> {
        #[cfg(test)]
        {
            let _ = (endpoint, form);
            Err("Real Microsoft network access is disabled in unit tests.".into())
        }
        #[cfg(not(test))]
        {
            let agent = microsoft_agent();
            let mut response = agent
                .post(format!("{AUTHORITY}/{endpoint}"))
                .send_form(form.iter().copied())
                .map_err(|e| transport_error(&e))?;
            let status = response.status().as_u16();
            if !(200..300).contains(&status) && !(400..500).contains(&status) {
                // Never accept success-shaped bodies attached to failed HTTP
                // responses or forward arbitrary redirect/server error text.
                return Err(http_status_error(status));
            }
            let body = response
                .body_mut()
                .with_config()
                .limit(65536)
                .read_to_string()
                .map_err(|e| transport_error(&e))?;
            checked_http_reply(status, Zeroizing::new(body))
        }
    }
}
// OAuth pending/denied errors use HTTP 400; retain those structured codes for
// the polling state machine, but require 2xx for token/device success payloads.
fn checked_http_reply(status: u16, body: Zeroizing<String>) -> Result<Zeroizing<String>, String> {
    if (200..300).contains(&status) {
        return Ok(body);
    }
    if matches!(status, 407 | 429) {
        return Err(http_status_error(status));
    }
    if (400..500).contains(&status) {
        let parsed = reply(&body).map_err(|_| http_status_error(status))?;
        if !parsed.error.is_empty() {
            return Ok(body);
        }
    }
    Err(http_status_error(status))
}

#[derive(Deserialize)]
struct Reply {
    #[serde(default)]
    error: String,
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    token_type: String,
    #[serde(default)]
    scope: String,
    #[serde(default)]
    expires_in: u64,
}
impl Drop for Reply {
    fn drop(&mut self) {
        self.access_token.zeroize();
        self.refresh_token.zeroize();
    }
}
fn error_message(code: &str) -> &'static str {
    match code {
        "authorization_declined" | "access_denied" => "Microsoft sign-in or consent was declined. No email was sent.",
        "expired_token" => "Microsoft sign-in code expired. Start sign-in again.",
        "invalid_grant" | "interaction_required" | "login_required" | "consent_required" => RECONNECT,
        "invalid_client" | "unauthorized_client" | "invalid_scope" => "Microsoft app registration does not permit this public-client SMTP flow. Check application ID, account audience, delegated SMTP.Send permission and public-client settings.",
        "temporarily_unavailable" | "server_error" => "Microsoft authorization is temporarily unavailable. Retry later.",
        _ => "Microsoft authorization failed. Check application registration, consent and account policy; provider details are redacted.",
    }
}
fn reply(raw: &str) -> Result<Reply, String> {
    serde_json::from_str(raw).map_err(|_| INVALID.into())
}
fn tokens(
    mut result: Reply,
    context: &Context,
    previous: Option<&str>,
    time: u64,
) -> Result<Tokens, String> {
    if !result.error.is_empty() {
        return Err(error_message(&result.error).into());
    }
    // When scope is omitted OAuth retains the requested scope. If provided,
    // require the SMTP resource permission; Graph Mail.Send is not SMTP.Send.
    if !result.token_type.eq_ignore_ascii_case("Bearer")
        || !secret_valid(&result.access_token)
        || result.expires_in <= 60
        || result.expires_in > 86400
        || (!result.scope.is_empty()
            && !result
                .scope
                .split_whitespace()
                .any(|s| s.eq_ignore_ascii_case("https://outlook.office.com/SMTP.Send")))
    {
        return Err(INVALID.into());
    }
    let refresh = if result.refresh_token.is_empty() {
        previous.ok_or(INVALID)?.to_owned()
    } else {
        std::mem::take(&mut result.refresh_token)
    };
    if !secret_valid(&refresh) {
        return Err(INVALID.into());
    }
    Ok(Tokens {
        context: context.clone(),
        access: std::mem::take(&mut result.access_token),
        refresh,
        expires_at: time.checked_add(result.expires_in).ok_or(INVALID)?,
    })
}
fn access_with(
    store: &dyn Store,
    http: &dyn Http,
    email: &EmailConfig,
    time: u64,
) -> Result<Zeroizing<String>, String> {
    validate(email)?;
    let context = Context::from_email(email)?;
    let id = email.microsoft_credential_ref.as_deref().ok_or(RECONNECT)?;
    let old = read(store, id, &context)?;
    if old.expires_at > time.saturating_add(60) {
        return Ok(Zeroizing::new(old.access.clone()));
    }
    let raw = http.post(
        "token",
        &[
            ("client_id", &context.client_id),
            ("grant_type", "refresh_token"),
            ("refresh_token", &old.refresh),
            ("scope", SCOPE),
        ],
    )?;
    let fresh = tokens(reply(&raw)?, &context, Some(&old.refresh), time)?;
    write(store, id, &fresh)?; // Never use a token whose refresh state could not be saved.
    Ok(Zeroizing::new(fresh.access.clone()))
}
pub(crate) fn for_send(email: &EmailConfig) -> Result<Zeroizing<String>, String> {
    let _guard = TOKENS.lock().map_err(|_| VAULT)?;
    access_with(
        crate::smtp_credentials::store(),
        &MicrosoftHttp,
        email,
        now()?,
    )
}

#[derive(Deserialize)]
struct Device {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    #[serde(default = "default_interval")]
    interval: u64,
}
fn default_interval() -> u64 {
    5
}
impl Drop for Device {
    fn drop(&mut self) {
        self.device_code.zeroize();
        self.user_code.zeroize();
    }
}
#[derive(Clone)]
pub(crate) struct Challenge {
    pub user_code: String,
    pub verification_uri: String,
}
impl Drop for Challenge {
    fn drop(&mut self) {
        self.user_code.zeroize();
    }
}
fn begin(http: &dyn Http, context: &Context) -> Result<Device, String> {
    let raw = http.post(
        "devicecode",
        &[("client_id", &context.client_id), ("scope", SCOPE)],
    )?;
    let value: Reply = reply(&raw)?;
    if !value.error.is_empty() {
        return Err(error_message(&value.error).into());
    }
    let device: Device = serde_json::from_str(&raw).map_err(|_| INVALID)?;
    // Never display a provider-controlled arbitrary link or human-readable error.
    if !matches!(
        device.verification_uri.as_str(),
        "https://microsoft.com/devicelogin" | "https://www.microsoft.com/link"
    ) || !secret_valid(&device.device_code)
        || device.user_code.len() > 32
        || device.user_code.is_empty()
        || !device
            .user_code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        || device.expires_in == 0
        || device.expires_in > 1800
        || device.interval > 60
    {
        return Err(INVALID.into());
    }
    Ok(device)
}
fn cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) {
        Err("Microsoft sign-in cancelled. No email was sent.".into())
    } else {
        Ok(())
    }
}
fn authorize(
    http: &dyn Http,
    store: &dyn Store,
    context: &Context,
    cancel: &AtomicBool,
    challenge: impl FnOnce(Challenge),
) -> Result<String, String> {
    authorize_with_wait(
        http,
        store,
        context,
        cancel,
        challenge,
        |interval, expires, start, cancel| {
            let next = Instant::now() + Duration::from_secs(interval);
            while Instant::now() < next {
                cancelled(cancel)?;
                if start.elapsed().as_secs() >= expires {
                    return Err(error_message("expired_token").into());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Ok(())
        },
    )
}
fn authorize_with_wait(
    http: &dyn Http,
    store: &dyn Store,
    context: &Context,
    cancel: &AtomicBool,
    challenge: impl FnOnce(Challenge),
    mut wait: impl FnMut(u64, u64, Instant, &AtomicBool) -> Result<(), String>,
) -> Result<String, String> {
    cancelled(cancel)?;
    let start = Instant::now();
    let device = begin(http, context)?;
    cancelled(cancel)?;
    challenge(Challenge {
        user_code: device.user_code.clone(),
        verification_uri: device.verification_uri.clone(),
    });
    let mut interval = device.interval.max(5);
    loop {
        wait(interval, device.expires_in, start, cancel)?;
        cancelled(cancel)?;
        if start.elapsed().as_secs() >= device.expires_in {
            return Err(error_message("expired_token").into());
        }
        let issued = now()?; // Conservative expiry: request start, not response receipt.
        let raw = http.post(
            "token",
            &[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", &context.client_id),
                ("device_code", &device.device_code),
            ],
        )?;
        cancelled(cancel)?;
        let result = reply(&raw)?;
        match result.error.as_str() {
            "authorization_pending" => continue,
            "slow_down" => {
                interval = interval.saturating_add(5);
                continue;
            }
            _ => (),
        }
        if start.elapsed().as_secs() >= device.expires_in {
            return Err(error_message("expired_token").into());
        }
        let tokens = tokens(result, context, None, issued)?;
        let _guard = TOKENS.lock().map_err(|_| VAULT)?;
        cancelled(cancel)?;
        let id = format!("oauth-v1-{}", uuid::Uuid::new_v4().simple());
        if store.get(&id).map_err(|_| VAULT)?.is_some() {
            return Err(VAULT.into());
        }
        write(store, &id, &tokens)?;
        cancelled(cancel)?; // A cancelled result is never attached to configuration.
        return Ok(id);
    }
}

enum Event {
    Challenge(Challenge),
    Complete(Result<String, String>),
}
#[derive(Default)]
pub(crate) struct SignInTask {
    receiver: Option<mpsc::Receiver<Event>>,
    cancel: Option<Arc<AtomicBool>>,
    context: Option<Context>,
    pub challenge: Option<Challenge>,
    pub status: String,
}
impl Drop for SignInTask {
    fn drop(&mut self) {
        self.cancel();
    }
}
impl SignInTask {
    pub fn pending(&self) -> bool {
        self.receiver.is_some()
    }
    pub fn cancel(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Release);
            self.challenge = None;
            self.status =
                "Cancellation requested. Waiting for the Microsoft sign-in worker to stop…".into();
        }
    }
    pub fn start(&mut self, email: &EmailConfig) -> Result<(), String> {
        if self.pending() {
            return Err("Microsoft sign-in is already running.".into());
        }
        let context = Context::from_email(email)?;
        let (sender, receiver) = mpsc::channel();
        #[cfg(test)]
        let _ = &receiver;
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let worker_context = context.clone();
        // Unit tests must inject protocol mocks; clicking a test UI cannot reach Microsoft.
        #[cfg(test)]
        {
            let _ = (sender, worker_cancel, worker_context);
            return Err("Real Microsoft sign-in is disabled in unit tests.".into());
        }
        #[cfg(not(test))]
        std::thread::Builder::new()
            .name("microsoft-sign-in".into())
            .spawn(move || {
                let result = authorize(
                    &MicrosoftHttp,
                    crate::smtp_credentials::store(),
                    &worker_context,
                    &worker_cancel,
                    |challenge| {
                        let _ = sender.send(Event::Challenge(challenge));
                    },
                );
                let _ = sender.send(Event::Complete(result));
            })
            .map_err(|_| "Could not start Microsoft sign-in worker.")?;
        #[allow(unreachable_code)]
        {
            self.receiver = Some(receiver);
            self.cancel = Some(cancel);
            self.context = Some(context);
            self.challenge = None;
            self.status = "Contacting Microsoft…".into();
            Ok(())
        }
    }
    pub fn poll(&mut self, email: &mut EmailConfig) {
        if self.pending() && Context::from_email(email).ok().as_ref() != self.context.as_ref() {
            self.cancel();
        }
        let Some(receiver) = &self.receiver else {
            return;
        };
        let event = match receiver.try_recv() {
            Ok(event) => event,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Event::Complete(Err(
                "Microsoft sign-in worker stopped. Retry sign-in.".into(),
            )),
        };
        match event {
            Event::Challenge(challenge) => {
                if self
                    .cancel
                    .as_ref()
                    .is_some_and(|cancel| cancel.load(Ordering::Acquire))
                {
                    return;
                }
                self.challenge = Some(challenge);
                self.status = "Open the Microsoft link and enter this code. Sign in to the configured mailbox and review consent.".into();
            }
            Event::Complete(result) => {
                let unchanged = Context::from_email(email).ok().as_ref() == self.context.as_ref();
                let cancelled = self
                    .cancel
                    .as_ref()
                    .is_some_and(|c| c.load(Ordering::Acquire));
                self.status = if cancelled || !unchanged {
                    "Microsoft sign-in cancelled or settings changed; result discarded. Start sign-in again.".into()
                } else {
                    match result {
                        Ok(id) => {
                            email.microsoft_credential_ref = Some(id);
                            "Microsoft sign-in complete. Save Email Settings, then explicitly send a test email. SMTP AUTH availability is checked when sending.".into()
                        }
                        Err(error) => error,
                    }
                };
                self.receiver = None;
                self.cancel = None;
                self.context = None;
                self.challenge = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::RefCell,
        collections::{HashMap, VecDeque},
    };
    #[derive(Debug)]
    struct FakeConnect {
        results: Mutex<VecDeque<Result<(), ureq::Error>>>,
        attempts: Mutex<Vec<std::net::SocketAddr>>,
    }
    impl ureq::unversioned::transport::Connector for FakeConnect {
        type Out = ();
        fn connect(
            &self,
            d: &ureq::unversioned::transport::ConnectionDetails,
            _: Option<()>,
        ) -> Result<Option<()>, ureq::Error> {
            assert_eq!(d.addrs.len(), 1);
            assert!(d.timeout.after <= ureq::unversioned::transport::time::Duration::from_secs(30));
            self.attempts.lock().unwrap().push(d.addrs[0]);
            self.results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap()
                .map(|_| Some(()))
        }
    }
    fn connection_attempts(
        results: Vec<Result<(), ureq::Error>>,
        v6_first: bool,
        seconds: u64,
    ) -> (Result<Option<()>, ureq::Error>, Vec<std::net::SocketAddr>) {
        use ureq::unversioned::{
            resolver::{DefaultResolver, Resolver},
            transport::{time, ConnectionDetails, Connector, NextTimeout},
        };
        let resolver = DefaultResolver::default();
        let mut addrs = resolver.empty();
        let v4 = "192.0.2.1:443".parse().unwrap();
        let v6 = "[2001:db8::1]:443".parse().unwrap();
        for a in if v6_first { [v6, v4] } else { [v4, v6] } {
            addrs.push(a);
        }
        let config = ureq::Agent::config_builder().build();
        let uri = "https://example.test".parse().unwrap();
        let details = ConnectionDetails {
            uri: &uri,
            addrs,
            config: &config,
            request_level: false,
            resolver: &resolver,
            now: time::Instant::now(),
            timeout: NextTimeout {
                after: time::Duration::from_secs(seconds),
                reason: ureq::Timeout::Global,
            },
            current_time: Arc::new(time::Instant::now),
            run_connector: Arc::new(|_| panic!("fake connection must not use network")),
        };
        let connector = AddressFallback(FakeConnect {
            results: Mutex::new(results.into()),
            attempts: Mutex::new(Vec::new()),
        });
        let result = connector.connect(&details, None);
        let attempts = connector.0.attempts.lock().unwrap().clone();
        (result, attempts)
    }
    const NATIVE_ID: &str = "oauth-v1-0123456789abcdef0123456789abcdef";
    const NATIVE_OTHER: &str = "oauth-v1-fedcba9876543210fedcba9876543210";

    #[test]
    #[ignore = "Child of native_synthetic_persistence_and_cleanup; no production entries"]
    fn native_synthetic_persistence_child() {
        let namespace = std::env::var("DPT_NATIVE_SYNTHETIC_NAMESPACE").unwrap();
        let store = crate::smtp_credentials::NativeAcceptanceStore::new(&namespace);
        assert_eq!(store.get("password").unwrap().as_deref(), Some("SYNTHETIC_ONLY"));
        let context = Context::from_email(&email()).unwrap();
        assert_eq!(read(&store, NATIVE_ID, &context).unwrap().access, "ACCESS_SECRET");
        assert!(read(&store, NATIVE_OTHER, &context).is_ok());
    }

    #[test]
    #[ignore = "Explicit native synthetic vault/process-restart/OAuth cleanup acceptance; no network"]
    fn native_synthetic_persistence_and_cleanup() {
        let namespace = uuid::Uuid::new_v4().to_string();
        let store = crate::smtp_credentials::NativeAcceptanceStore::new(&namespace);
        assert!(store.get("password").unwrap().is_none());
        store.put("password", "SYNTHETIC_ONLY").unwrap();
        let mut e = email();
        let context = Context::from_email(&e).unwrap();
        let tokens = tokens(reply(TOKEN).unwrap(), &context, None, 100).unwrap();
        let mut generations = Vec::new();
        for _ in 0..2 {
            write(&store, NATIVE_ID, &tokens).unwrap();
            generations.push(serde_json::from_str::<Manifest>(&store.get(NATIVE_ID).unwrap().unwrap()).unwrap());
        }
        write(&store, NATIVE_OTHER, &tokens).unwrap();
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "microsoft_oauth::tests::native_synthetic_persistence_child", "--ignored", "--test-threads=1"])
            .env("DPT_NATIVE_SYNTHETIC_NAMESPACE", &namespace).output().unwrap();
        assert!(child.status.success(), "synthetic persistence child failed (details withheld)");
        e.microsoft_credential_ref = Some(NATIVE_ID.into());
        cleanup_with(&store, &e).unwrap();
        assert!(store.get(NATIVE_ID).unwrap().is_none());
        for manifest in generations {
            for part in 0..manifest.parts {
                assert!(store.get(&format!("oauth-part-v1-{}-{part}", manifest.generation)).unwrap().is_none());
            }
        }
        assert!(read(&store, NATIVE_ID, &context).is_err());
        assert!(write(&store, NATIVE_ID, &tokens).is_err());
        assert!(read(&store, NATIVE_OTHER, &context).is_ok());
        cleanup_with(&store, &e).unwrap();
        store.delete("password").unwrap();
        assert!(store.get("password").unwrap().is_none());
        // Drop verifies deletion of every remaining fixture-owned entry.
    }

    #[test]
    fn local_cleanup_removes_indexed_generations_and_blocks_restored_references() {
        let store = Memory::default();
        let mut e = email();
        let context = Context::from_email(&e).unwrap();
        let tokens = tokens(reply(TOKEN).unwrap(), &context, None, 100).unwrap();
        let id = "oauth-v1-0123456789abcdef0123456789abcdef";
        let other = "oauth-v1-fedcba9876543210fedcba9876543210";
        write(&store, id, &tokens).unwrap();
        write(&store, other, &tokens).unwrap();
        let first: Manifest = serde_json::from_str(&store.get(id).unwrap().unwrap()).unwrap();
        write(&store, id, &tokens).unwrap();
        e.microsoft_credential_ref = Some(id.into());
        cleanup_with(&store, &e).unwrap();
        assert!(store.get(id).unwrap().is_none());
        assert!(store
            .get(&format!("oauth-part-v1-{}-0", first.generation))
            .unwrap()
            .is_none());
        assert!(read(&store, other, &context).is_ok());
        assert!(read(&store, id, &context).is_err());
        assert!(write(&store, id, &tokens).is_err());
        cleanup_with(&store, &e).unwrap();
    }
    #[test]
    fn interrupted_cleanup_is_fail_closed_and_retryable() {
        struct RefuseDelete<'a>(&'a Memory);
        impl Store for RefuseDelete<'_> {
            fn get(&self, id: &str) -> Result<Option<String>, String> {
                self.0.get(id)
            }
            fn put(&self, id: &str, v: &str) -> Result<(), String> {
                self.0.put(id, v)
            }
            fn delete(&self, _: &str) -> Result<(), String> {
                Err("SECRET_BACKEND_ERROR".into())
            }
        }
        let store = Memory::default();
        let mut e = email();
        let context = Context::from_email(&e).unwrap();
        let t = tokens(reply(TOKEN).unwrap(), &context, None, 100).unwrap();
        let id = "oauth-v1-0123456789abcdef0123456789abcdef";
        write(&store, id, &t).unwrap();
        e.microsoft_credential_ref = Some(id.into());
        let error = cleanup_with(&RefuseDelete(&store), &e).unwrap_err();
        assert!(!error.contains("SECRET"));
        assert!(read(&store, id, &context).is_err());
        cleanup_with(&store, &e).unwrap();
    }
    #[test]
    fn cleanup_retries_after_manifest_and_first_chunk_were_removed() {
        struct FailSecond<'a>(&'a Memory);
        impl Store for FailSecond<'_> {
            fn get(&self, id: &str) -> Result<Option<String>, String> {
                self.0.get(id)
            }
            fn put(&self, id: &str, v: &str) -> Result<(), String> {
                self.0.put(id, v)
            }
            fn delete(&self, id: &str) -> Result<(), String> {
                if id.ends_with("-1") {
                    Err("SECRET".into())
                } else {
                    self.0.delete(id)
                }
            }
        }
        let store = Memory::default();
        let mut e = email();
        let context = Context::from_email(&e).unwrap();
        let mut t = tokens(reply(TOKEN).unwrap(), &context, None, 100).unwrap();
        t.access = "A".repeat(4000);
        let id = "oauth-v1-0123456789abcdef0123456789abcdef";
        write(&store, id, &t).unwrap();
        e.microsoft_credential_ref = Some(id.into());
        assert!(cleanup_with(&FailSecond(&store), &e).is_err());
        assert!(store.get(id).unwrap().is_none());
        assert!(read(&store, id, &context).is_err());
        assert!(write(&store, id, &t).is_err());
        cleanup_with(&store, &e).unwrap();
        cleanup_with(&store, &e).unwrap();
    }
    #[test]
    fn cleanup_rejects_wrong_account_and_corrupt_chain_without_deleting_chunks() {
        let store = Memory::default();
        let mut e = email();
        let context = Context::from_email(&e).unwrap();
        let t = tokens(reply(TOKEN).unwrap(), &context, None, 100).unwrap();
        let id = "oauth-v1-0123456789abcdef0123456789abcdef";
        write(&store, id, &t).unwrap();
        e.microsoft_credential_ref = Some(id.into());
        let mut wrong = e.clone();
        wrong.smtp_username = "another@example.test".into();
        assert!(cleanup_with(&store, &wrong).is_err());
        assert!(read(&store, id, &context).is_ok());
        let m: Manifest = serde_json::from_str(&store.get(id).unwrap().unwrap()).unwrap();
        store
            .put(&generation_key(&m.generation), "invalid metadata")
            .unwrap();
        assert!(cleanup_with(&store, &e).is_err());
        assert!(store.get(id).unwrap().is_some());
        assert!(store
            .get(&format!("oauth-part-v1-{}-0", m.generation))
            .unwrap()
            .is_some());
    }
    #[test]
    fn unreachable_family_falls_back_before_http_in_both_directions() {
        for (kind, v6_first) in [
            (std::io::ErrorKind::HostUnreachable, true),
            (std::io::ErrorKind::NetworkUnreachable, false),
            (std::io::ErrorKind::ConnectionRefused, true),
        ] {
            let (result, attempts) = connection_attempts(
                vec![
                    Err(ureq::Error::Io(std::io::Error::new(
                        kind,
                        "SECRET_INTERNAL_DETAIL",
                    ))),
                    Ok(()),
                ],
                v6_first,
                30,
            );
            assert!(result.is_ok());
            assert_eq!(attempts.len(), 2);
            assert_eq!(attempts[0].is_ipv6(), v6_first);
            assert_ne!(attempts[0].is_ipv6(), attempts[1].is_ipv6());
        }
    }
    #[test]
    fn address_fallback_preserves_tls_proxy_errors_and_deadline() {
        for error in [
            ureq::Error::Tls("SECRET_CERT_DETAIL"),
            ureq::Error::ConnectProxyFailed("SECRET_PROXY_CREDENTIAL".into()),
            ureq::Error::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "SECRET",
            )),
        ] {
            let (result, attempts) = connection_attempts(vec![Err(error)], true, 30);
            assert!(result.is_err());
            assert_eq!(attempts.len(), 1);
        }
        let (result, attempts) = connection_attempts(vec![], true, 0);
        assert!(matches!(result, Err(ureq::Error::Timeout(_))));
        assert!(attempts.is_empty());
        let (result, attempts) = connection_attempts(
            vec![Err(ureq::Error::Timeout(ureq::Timeout::Connect)), Ok(())],
            true,
            30,
        );
        assert!(result.is_ok());
        assert_eq!(attempts.len(), 2);
    }
    #[test]
    fn actionable_transport_errors_never_format_sensitive_details() {
        for (error, expected) in [
            (ureq::Error::HostNotFound, "DNS"),
            (ureq::Error::Tls("SECRET_CERT_DETAIL"), "TLS"),
            (
                ureq::Error::ConnectProxyFailed("SECRET_PROXY_CREDENTIAL".into()),
                "proxy",
            ),
            (
                ureq::Error::Io(std::io::Error::new(
                    std::io::ErrorKind::HostUnreachable,
                    "SECRET",
                )),
                "route",
            ),
            (ureq::Error::Timeout(ureq::Timeout::Global), "timed out"),
        ] {
            let text = transport_error(&error);
            assert!(text.contains(expected), "{text}");
            assert!(!text.contains("SECRET"));
        }
    }
    #[test]
    fn http_failures_are_distinct_and_bodies_remain_redacted() {
        for (status, expected) in [
            (302, "redirect"),
            (407, "proxy"),
            (429, "rate limit"),
            (503, "gateway"),
            (403, "HTTP 403"),
        ] {
            let error =
                checked_http_reply(status, Zeroizing::new("SECRET_BODY".into())).unwrap_err();
            assert!(error.contains(expected), "{error}");
            assert!(!error.contains("SECRET"));
        }
    }
    #[test]
    fn sign_in_worker_error_reaches_ui_without_replacing_saved_authorization() {
        let mut email = email();
        let saved = "oauth-v1-0123456789abcdef0123456789abcdef";
        email.microsoft_credential_ref = Some(saved.into());
        let mut task = SignInTask::default();
        let (tx, rx) = mpsc::channel();
        task.receiver = Some(rx);
        task.context = Some(Context::from_email(&email).unwrap());
        task.cancel = Some(Arc::new(AtomicBool::new(false)));
        let message = transport_error(&ureq::Error::HostNotFound);
        tx.send(Event::Complete(Err(message.into()))).unwrap();
        task.poll(&mut email);
        assert_eq!(task.status, message);
        assert!(!task.pending());
        assert!(task.challenge.is_none());
        assert_eq!(email.microsoft_credential_ref.as_deref(), Some(saved));
    }
    #[derive(Default)]
    struct Memory {
        values: RefCell<HashMap<String, String>>,
        fail_write: bool,
        corrupt: bool,
    }
    impl Store for Memory {
        fn delete(&self, id: &str) -> Result<(), String> {
            self.values.borrow_mut().remove(id);
            Ok(())
        }
        fn get(&self, id: &str) -> Result<Option<String>, String> {
            Ok(self.values.borrow().get(id).cloned())
        }
        fn put(&self, id: &str, value: &str) -> Result<(), String> {
            if self.fail_write {
                return Err("SECRET must never escape".into());
            }
            self.values.borrow_mut().insert(
                id.into(),
                if self.corrupt {
                    "invalid".into()
                } else {
                    value.into()
                },
            );
            Ok(())
        }
    }
    struct MockHttp {
        replies: RefCell<VecDeque<String>>,
        requests: RefCell<Vec<(String, Vec<(String, String)>)>>,
    }
    impl MockHttp {
        fn new(replies: &[&str]) -> Self {
            Self {
                replies: RefCell::new(replies.iter().map(|s| s.to_string()).collect()),
                requests: RefCell::new(Vec::new()),
            }
        }
    }
    impl Http for MockHttp {
        fn post(&self, endpoint: &str, form: &[(&str, &str)]) -> Result<Zeroizing<String>, String> {
            self.requests.borrow_mut().push((
                endpoint.into(),
                form.iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            ));
            Ok(Zeroizing::new(
                self.replies
                    .borrow_mut()
                    .pop_front()
                    .expect("unexpected HTTP request"),
            ))
        }
    }
    fn email() -> EmailConfig {
        let mut e = EmailConfig::default();
        e.smtp_transport = "SMTP Server".into();
        e.smtp_authentication = SmtpAuthentication::MicrosoftOAuth2;
        e.smtp_host = "smtp-mail.outlook.com".into();
        e.smtp_port = 587;
        e.smtp_username = "fixture@outlook.com".into();
        e.microsoft_client_id = "12345678-1234-1234-1234-123456789abc".into();
        e
    }
    const DEVICE: &str = r#"{"device_code":"DEVICE_SECRET","user_code":"ABCD-EFGH","verification_uri":"https://microsoft.com/devicelogin","expires_in":900,"interval":5}"#;
    const TOKEN: &str = r#"{"access_token":"ACCESS_SECRET","refresh_token":"REFRESH_SECRET","token_type":"Bearer","expires_in":3600,"scope":"https://outlook.office.com/SMTP.Send"}"#;
    fn seed(store: &Memory, e: &mut EmailConfig, expiry: u64) -> String {
        let id = format!("oauth-v1-{}", uuid::Uuid::new_v4().simple());
        let record = Tokens {
            context: Context::from_email(e).unwrap(),
            access: "OLD_ACCESS".into(),
            refresh: "OLD_REFRESH".into(),
            expires_at: expiry,
        };
        write(store, &id, &record).unwrap();
        e.microsoft_credential_ref = Some(id.clone());
        id
    }
    #[test]
    fn mandatory_microsoft_oauth_does_not_change_other_provider_or_local_relay() {
        for domain in ["outlook.com", "HOTMAIL.COM", "live.com", "msn.com"] {
            let mut e = email();
            e.smtp_username = format!("fixture@{domain}");
            e.smtp_authentication = SmtpAuthentication::Password;
            assert!(validate(&e).unwrap_err().contains("requires OAuth2"));
        }
        for host in ["smtp.gmx.com", "smtp.gmail.com", "relay.example.invalid"] {
            let mut e = EmailConfig::default();
            e.smtp_transport = "SMTP Server".into();
            e.smtp_host = host.into();
            e.smtp_username = "fixture@example.invalid".into();
            assert!(validate(&e).is_ok());
        }
        let mut e = email();
        e.smtp_transport = "Local SMTP Server".into();
        assert!(validate(&e).is_ok());
    }
    #[test]
    fn oauth_rejects_non_tls_wrong_endpoint_and_control_characters() {
        let mut e = email();
        for security in [
            None,
            Some(SmtpSecurity::Plaintext),
            Some(SmtpSecurity::ImplicitTls),
        ] {
            e.smtp_security = security;
            assert!(Context::from_email(&e).is_err());
        }
        e = email();
        e.smtp_port = 465;
        assert!(Context::from_email(&e).is_err());
        e = email();
        e.smtp_host = "attacker.invalid".into();
        assert!(Context::from_email(&e).is_err());
        e = email();
        e.smtp_username = "fixture\u{1}auth=Bearer@outlook.com".into();
        assert!(Context::from_email(&e).is_err());
        e = email();
        e.microsoft_client_id.clear();
        assert!(Context::from_email(&e).is_err());
        assert!(!secret_valid("secret\r\nAUTH"));
    }
    #[test]
    fn device_flow_pending_slow_down_and_success_stores_only_vault_reference() {
        let e = email();
        let context = Context::from_email(&e).unwrap();
        let store = Memory::default();
        let http = MockHttp::new(&[
            DEVICE,
            r#"{"error":"authorization_pending"}"#,
            r#"{"error":"slow_down"}"#,
            TOKEN,
        ]);
        let mut intervals = Vec::new();
        let mut shown = false;
        let id = authorize_with_wait(
            &http,
            &store,
            &context,
            &AtomicBool::new(false),
            |challenge| {
                assert_eq!(challenge.user_code, "ABCD-EFGH");
                shown = true;
            },
            |interval, _, _, _| {
                intervals.push(interval);
                Ok(())
            },
        )
        .unwrap();
        assert!(shown);
        assert_eq!(intervals, [5, 5, 10]);
        assert_eq!(
            read(&store, &id, &context).unwrap().refresh,
            "REFRESH_SECRET"
        );
        let requests = http.requests.borrow();
        assert_eq!(requests[0].0, "devicecode");
        assert!(requests[0].1.contains(&("scope".into(), SCOPE.into())));
        assert!(requests[1].1.contains(&(
            "grant_type".into(),
            "urn:ietf:params:oauth:grant-type:device_code".into()
        )));
        let mut saved = e;
        saved.microsoft_credential_ref = Some(id);
        for text in [
            toml::to_string(&saved).unwrap(),
            serde_json::to_string(&saved).unwrap(),
            format!("{saved:?}"),
        ] {
            for secret in [
                "ACCESS_SECRET",
                "REFRESH_SECRET",
                "DEVICE_SECRET",
                "ABCD-EFGH",
            ] {
                assert!(!text.contains(secret));
            }
        }
    }
    #[test]
    fn cached_access_requires_fresh_vault_retrieval_and_context_match() {
        let store = Memory::default();
        let mut e = email();
        let id = seed(&store, &mut e, 5000);
        let http = MockHttp::new(&[]);
        assert_eq!(
            access_with(&store, &http, &e, 1000).unwrap().as_str(),
            "OLD_ACCESS"
        );
        assert!(http.requests.borrow().is_empty());
        e.smtp_username = "other@hotmail.com".into();
        assert!(access_with(&store, &http, &e, 1000).is_err());
        e = email();
        e.microsoft_credential_ref = Some(id.clone());
        store.values.borrow_mut().remove(&id);
        assert!(access_with(&store, &http, &e, 1000).is_err());
    }
    #[test]
    fn expiry_refresh_rotates_and_verifies_tokens_before_use() {
        let store = Memory::default();
        let mut e = email();
        let id = seed(&store, &mut e, 1000);
        let http = MockHttp::new(&[TOKEN]);
        assert_eq!(
            access_with(&store, &http, &e, 1000).unwrap().as_str(),
            "ACCESS_SECRET"
        );
        let record = read(&store, &id, &Context::from_email(&e).unwrap()).unwrap();
        assert_eq!(record.refresh, "REFRESH_SECRET");
        assert_eq!(record.expires_at, 4600);
        assert!(http.requests.borrow()[0]
            .1
            .contains(&("refresh_token".into(), "OLD_REFRESH".into())));
    }
    #[test]
    fn refresh_without_rotation_keeps_previous_refresh_token() {
        let store = Memory::default();
        let mut e = email();
        let id = seed(&store, &mut e, 1);
        let http = MockHttp::new(&[
            r#"{"access_token":"NEW_ACCESS","token_type":"Bearer","expires_in":3600}"#,
        ]);
        assert!(access_with(&store, &http, &e, 1000).is_ok());
        assert_eq!(
            read(&store, &id, &Context::from_email(&e).unwrap())
                .unwrap()
                .refresh,
            "OLD_REFRESH"
        );
    }
    #[test]
    fn store_write_or_readback_failure_prevents_access_and_redacts_backend_error() {
        for corrupt in [false, true] {
            let mut store = Memory::default();
            let mut e = email();
            seed(&store, &mut e, 1);
            store.fail_write = !corrupt;
            store.corrupt = corrupt;
            let error = access_with(&store, &MockHttp::new(&[TOKEN]), &e, 1000).unwrap_err();
            assert!(!error.contains("SECRET"));
            assert!(error.contains("credential store"));
        }
    }
    #[test]
    fn revoked_refresh_fails_closed_without_basic_fallback_or_store_change() {
        let store = Memory::default();
        let mut e = email();
        let id = seed(&store, &mut e, 1);
        let before = store.values.borrow()[&id].clone();
        let http = MockHttp::new(&[r#"{"error":"invalid_grant","error_description":"SECRET"}"#]);
        let error = access_with(&store, &http, &e, 1000).unwrap_err();
        assert!(error.contains("reconnect"));
        assert!(!error.contains("SECRET"));
        assert_eq!(store.values.borrow()[&id], before);
        assert_eq!(http.requests.borrow().len(), 1);
    }
    #[test]
    fn cancellation_decline_expiry_and_bad_links_never_publish_credentials() {
        let store = Memory::default();
        let context = Context::from_email(&email()).unwrap();
        let http = MockHttp::new(&[]);
        assert!(authorize(
            &http,
            &store,
            &context,
            &AtomicBool::new(true),
            |_| panic!()
        )
        .is_err());
        let cancel = AtomicBool::new(false);
        assert!(authorize_with_wait(
            &MockHttp::new(&[DEVICE]),
            &store,
            &context,
            &cancel,
            |_| cancel.store(true, Ordering::Release),
            |_, _, _, cancel| cancelled(cancel)
        )
        .is_err());
        for code in [
            "authorization_declined",
            "expired_token",
            "invalid_client",
            "invalid_scope",
        ] {
            let response = format!(r#"{{"error":"{code}","error_description":"SECRET"}}"#);
            let http = MockHttp::new(&[DEVICE, &response]);
            let error = authorize_with_wait(
                &http,
                &store,
                &context,
                &AtomicBool::new(false),
                |_| (),
                |_, _, _, _| Ok(()),
            )
            .unwrap_err();
            assert!(!error.contains("SECRET"));
        }
        let bad = DEVICE.replace(
            "https://microsoft.com/devicelogin",
            "https://attacker.invalid/",
        );
        assert!(begin(&MockHttp::new(&[&bad]), &context).is_err());
        assert!(store.values.borrow().is_empty());
    }
    #[test]
    fn malformed_tokens_and_wrong_scopes_are_refused() {
        let context = Context::from_email(&email()).unwrap();
        for response in [
            TOKEN.replace("Bearer", "Basic"),
            TOKEN.replace("3600", "0"),
            TOKEN.replace(
                "https://outlook.office.com/SMTP.Send",
                "https://graph.microsoft.com/Mail.Send",
            ),
            TOKEN.replace("REFRESH_SECRET", ""),
        ] {
            assert!(tokens(reply(&response).unwrap(), &context, None, 1000).is_err());
        }
        assert_eq!(
            reply(r#"{"error":"access_denied"}"#).unwrap().error,
            "access_denied"
        );
    }
    #[test]
    fn stale_or_cancelled_ui_completion_does_not_attach_reference() {
        for cancellation in [false, true] {
            let mut e = email();
            let context = Context::from_email(&e).unwrap();
            let (tx, rx) = mpsc::channel();
            let cancel = Arc::new(AtomicBool::new(cancellation));
            let mut task = SignInTask::default();
            task.receiver = Some(rx);
            task.cancel = Some(cancel);
            task.context = Some(context);
            if !cancellation {
                e.smtp_username = "changed@live.com".into();
            }
            tx.send(Event::Complete(Ok(
                "oauth-v1-0123456789abcdef0123456789abcdef".into(),
            )))
            .unwrap();
            task.poll(&mut e);
            assert!(e.microsoft_credential_ref.is_none());
            assert!(!task.pending());
        }
    }
    #[test]
    fn config_and_backup_copies_reject_nested_plaintext_oauth_secrets() {
        for field in [
            "access_token",
            "refresh_token",
            "device_code",
            "user_code",
            "authorization_code",
            "auth_code",
            "code_verifier",
            "verification_uri_complete",
            "client_secret",
            "id_token",
        ] {
            let source = format!("[email]\n{field} = 'SECRET'\n");
            let value: toml::Value = toml::from_str(&source).unwrap();
            let error = crate::smtp_credentials::reject_oauth_secrets(&value).unwrap_err();
            assert!(!error.contains("SECRET"));
            assert!(crate::smtp_credentials::protected_bytes(source.as_bytes()).is_err());
        }
        let source =
            b"[email]\nmicrosoft_client_id = 'public'\nmicrosoft_credential_ref = 'opaque'\n";
        assert_eq!(
            crate::smtp_credentials::protected_bytes(source).unwrap(),
            source
        );
    }
    #[test]
    fn lettre_xoauth2_wire_response_is_exact_and_distinct_from_password_auth() {
        use lettre::transport::smtp::authentication::{Credentials, Mechanism};
        let response = Mechanism::Xoauth2
            .response(
                &Credentials::new("fixture@outlook.com".into(), "ACCESS_SECRET".into()),
                None,
            )
            .unwrap();
        assert_eq!(
            response,
            "user=fixture@outlook.com\u{1}auth=Bearer ACCESS_SECRET\u{1}\u{1}"
        );
    }
    #[test]
    fn test_build_cannot_contact_microsoft() {
        assert!(MicrosoftHttp.post("token", &[]).is_err());
        assert!(SignInTask::default().start(&email()).is_err());
    }
    #[test]
    fn large_vault_records_fit_native_entry_limits_and_verify_every_chunk() {
        let store = Memory::default();
        let e = email();
        let context = Context::from_email(&e).unwrap();
        let id = format!("oauth-v1-{}", uuid::Uuid::new_v4().simple());
        let record = Tokens {
            context: context.clone(),
            access: "A".repeat(6000),
            refresh: "R".repeat(4000),
            expires_at: 5000,
        };
        write(&store, &id, &record).unwrap();
        assert!(store
            .values
            .borrow()
            .values()
            .all(|value| value.len() <= PART_SIZE));
        assert!(!store.values.borrow()[&id].contains("AAAA"));
        assert_eq!(read(&store, &id, &context).unwrap().access.len(), 6000);
        let key = store
            .values
            .borrow()
            .keys()
            .find(|key| key.starts_with("oauth-part-v1-"))
            .unwrap()
            .clone();
        store.values.borrow_mut().remove(&key);
        assert!(read(&store, &id, &context).is_err());
    }
    #[test]
    fn ui_completion_requires_explicit_poll_and_retains_only_reference() {
        let mut e = email();
        let context = Context::from_email(&e).unwrap();
        let (tx, rx) = mpsc::channel();
        let mut task = SignInTask::default();
        task.receiver = Some(rx);
        task.context = Some(context);
        task.cancel = Some(Arc::new(AtomicBool::new(false)));
        tx.send(Event::Challenge(Challenge {
            user_code: "ONE-TIME".into(),
            verification_uri: "https://microsoft.com/devicelogin".into(),
        }))
        .unwrap();
        task.poll(&mut e);
        assert!(task.pending());
        assert!(task.challenge.is_some());
        assert!(e.microsoft_credential_ref.is_none());
        let id = "oauth-v1-0123456789abcdef0123456789abcdef";
        tx.send(Event::Complete(Ok(id.into()))).unwrap();
        task.poll(&mut e);
        assert_eq!(e.microsoft_credential_ref.as_deref(), Some(id));
        assert!(!task.pending());
        assert!(task.challenge.is_none());
        assert!(!toml::to_string(&e).unwrap().contains("ONE-TIME"));
    }
    #[test]
    fn cancel_hides_code_and_discards_a_queued_challenge_and_completion() {
        let mut e = email();
        let (tx, rx) = mpsc::channel();
        let mut task = SignInTask::default();
        task.receiver = Some(rx);
        task.context = Some(Context::from_email(&e).unwrap());
        task.cancel = Some(Arc::new(AtomicBool::new(false)));
        task.challenge = Some(Challenge {
            user_code: "OLD-CODE".into(),
            verification_uri: "https://microsoft.com/devicelogin".into(),
        });
        task.cancel();
        assert!(task.challenge.is_none());
        tx.send(Event::Challenge(Challenge {
            user_code: "QUEUED-CODE".into(),
            verification_uri: "https://microsoft.com/devicelogin".into(),
        }))
        .unwrap();
        task.poll(&mut e);
        assert!(task.challenge.is_none());
        assert!(task.pending());
        tx.send(Event::Complete(Ok(
            "oauth-v1-0123456789abcdef0123456789abcdef".into(),
        )))
        .unwrap();
        task.poll(&mut e);
        assert!(e.microsoft_credential_ref.is_none());
        assert!(!task.pending());
    }
    #[test]
    fn oauth_settings_preserve_dormant_password_reference_without_retrieving_it() {
        let mut e = email();
        e.smtp_credential_ref = Some("v1-0123456789abcdef0123456789abcdef".into());
        e.smtp_credential_error = Some("Dormant password vault is unavailable".into());
        e.smtp_password = "DORMANT_SECRET".into();
        e.microsoft_credential_ref = Some("oauth-v1-0123456789abcdef0123456789abcdef".into());
        assert_eq!(
            crate::smtp_credentials::prepare_save(&e).unwrap(),
            e.smtp_credential_ref
        );
        let serialized = toml::to_string(&e).unwrap();
        assert!(!serialized.contains("DORMANT_SECRET"));
        let restored: EmailConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(
            restored.smtp_authentication,
            SmtpAuthentication::MicrosoftOAuth2
        );
        assert_eq!(
            restored.microsoft_credential_ref,
            e.microsoft_credential_ref
        );
        assert!(restored.smtp_password.is_empty());
        assert!(crate::email_settings::validate_standalone_test(
            &restored,
            Some(crate::email_settings::TestRecipient::Payroll)
        )
        .unwrap_err()
        .contains("test recipient"));
    }
    #[test]
    fn dependency_wire_diagnostics_are_compiled_out_even_in_debug_builds() {
        assert!(log::STATIC_MAX_LEVEL <= log::LevelFilter::Info);
        let formatted = std::cell::Cell::new(false);
        log::trace!("{}", {
            formatted.set(true);
            "ACCESS_REFRESH_DEVICE_USER_CODE_SECRET"
        });
        log::debug!("{}", {
            formatted.set(true);
            "AUTH_XOAUTH2_SECRET"
        });
        assert!(!formatted.get());
    }
    #[test]
    fn http_failure_status_cannot_publish_a_success_shaped_payload() {
        for status in [301, 400, 401, 403, 429, 500, 503] {
            assert!(checked_http_reply(status, Zeroizing::new(TOKEN.into())).is_err());
            assert!(checked_http_reply(status, Zeroizing::new(DEVICE.into())).is_err());
        }
        let pending = r#"{"error":"authorization_pending","error_description":"SECRET"}"#;
        assert!(checked_http_reply(400, Zeroizing::new(pending.into())).is_ok());
        assert!(checked_http_reply(503, Zeroizing::new(pending.into())).is_err());
        assert!(checked_http_reply(200, Zeroizing::new(TOKEN.into())).is_ok());
    }
    #[test]
    fn nested_codes_are_rejected_before_config_load_or_backup_publication() {
        let dir = tempfile::tempdir().unwrap();
        for field in [
            "USER_CODE",
            "authorization_code",
            "code_verifier",
            "verification_uri_complete",
        ] {
            let source = format!("[[extensions]]\n{field} = 'ONE_TIME_SECRET'\n");
            let path = dir.path().join("config.toml");
            std::fs::write(&path, &source).unwrap();
            let error = crate::config::AppConfig::load(&path)
                .unwrap_err()
                .to_string();
            assert!(!error.contains("ONE_TIME_SECRET"));
            assert!(!error.contains(field));
            assert!(crate::smtp_credentials::protected_bytes(source.as_bytes()).is_err());
            assert_eq!(std::fs::read_to_string(path).unwrap(), source);
        }
    }
}
