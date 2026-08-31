//! `AttemptService` — attempt entry, the Tier A capability machinery, the
//! Tier B session-code gates, and the attempt pool
//! (hand-written, user-owned; see `metaphor.codegen.yaml`).
//!
//! ADR-0018 re-declared on the survey shapes:
//!
//! - **Tier A (the per-attempt machine capability)** — the public link
//!   carries `{input_id}.{nonce}.{exp}.{mac}`; the MAC is HMAC-SHA256
//!   over `(id, nonce, grant, exp)` where the grant binds the capability
//!   to its survey (`survey_input:{survey_id}`). A leaked row alone is
//!   inert (the nonce is a selector, not the capability). MULTI-USE
//!   within the attempt's life (unlike rating's single-use submit):
//!   begin/submit/next/certification all ride it until `state = done` or
//!   expiry. Rotation mints fresh nonce + expiry atomically; the old
//!   nonce dies with the UPDATE. Verification failures share ONE refusal
//!   (unknown / expired / malformed / done are indistinguishable in the
//!   body; expiry is additionally a typed 410 by the spec's own
//!   carving). The survey-level `access_token` stays a plain public URL
//!   key — participants are NEVER authorized by it.
//! - **Tier B (the session code)** — short by UX necessity, so the other
//!   knobs are all present (the attendance kiosk-PIN precedent):
//!   state-based + 24 h hard-TTL expiry, 4→9 digit mint ladder with loud
//!   exhaustion, escalating lockout (3 failures → 30 s doubling to a
//!   15 min cap, 1 s minimum spacing) per identity AND per IP, success
//!   resets. The lockout book is in-memory per composing service (the
//!   kiosk-posture precedent; a multi-instance host fronts it with its
//!   own shared limiter — the same trade the attendance module ships).
//! - **The pool** — attempt counting is the raw-SQL self-join (same
//!   survey + done + not-test + shared-or-NULL invite token + same
//!   partner-or-email), flushed inside the caller's transaction.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;
use sqlx::PgPool;
use uuid::Uuid;

use crate::application::service::event_sink::{EventSinkSlot, Fact};
use crate::application::service::survey_write_service::{
    session_armed_at, session_code_live, SurveyWriteError,
};
use crate::domain::entity::{Survey, SurveyInputState, SurveyInviteExistingMode, UserInput};
use crate::infrastructure::persistence::attempt_repository::AttemptRepository;
use crate::infrastructure::persistence::scoring_repository::ScoringRepository;
use crate::infrastructure::persistence::survey_session_repository::SurveySessionRepository;

/// Env var holding the HMAC secret for attempt tokens.
pub const SURVEY_TOKEN_SECRET_ENV: &str = "SURVEY_TOKEN_SECRET";

/// Default token lifetime at entry (30 days, config-overridable upstream).
pub const DEFAULT_TOKEN_TTL_DAYS: i64 = 30;

// ─── Tier B policy knobs (probe-asserted as pure functions) ───────────────────

/// Consecutive failures before the first lockout kicks in.
pub const CODE_MAX_FAILURES: i32 = 3;
/// First lockout duration; doubles per extra failure.
pub const CODE_LOCK_BASE_SECONDS: i64 = 30;
/// Ceiling for the escalating lockout (15 minutes).
pub const CODE_LOCK_CAP_SECONDS: i64 = 900;
/// Minimum spacing between code attempts (anti-hammering).
pub const CODE_ATTEMPT_SPACING: Duration = Duration::seconds(1);

/// Escalating lockout: `failures` have now accumulated (pass the
/// post-increment count). Lock starts at [`CODE_MAX_FAILURES`], doubles
/// per extra failure, caps at [`CODE_LOCK_CAP_SECONDS`]. Pure — the
/// probes assert the table directly. `None` = not locked yet.
pub fn lockout_until(failures: i32, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    if failures < CODE_MAX_FAILURES {
        return None;
    }
    let doubles = (failures - CODE_MAX_FAILURES) as u32;
    let secs = CODE_LOCK_BASE_SECONDS.saturating_mul(1i64 << doubles.min(16));
    Some(now + Duration::seconds(secs.min(CODE_LOCK_CAP_SECONDS)))
}

// ─── Tier A link machinery (the rating precedent, multi-use) ──────────────────

type HmacSha256 = Hmac<Sha256>;

/// The parsed public capability link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCapability {
    pub id: Uuid,
    pub nonce: String,
    pub exp: i64,
    pub mac: String,
}

/// Parse `{id}.{nonce}.{exp}.{mac}` — anything else is malformed (and
/// malformed is refusal-shaped, never a panic).
pub fn parse_capability(link: &str) -> Option<ParsedCapability> {
    let mut parts = link.split('.');
    let id = Uuid::parse_str(parts.next()?).ok()?;
    let nonce = parts.next()?.to_string();
    if nonce.is_empty() || !nonce.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let exp: i64 = parts.next()?.parse().ok()?;
    let mac = parts.next()?.to_string();
    if mac.len() != 64 || !mac.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    if parts.next().is_some() {
        return None;
    }
    Some(ParsedCapability { id, nonce, exp, mac })
}

fn mint_nonce() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The MAC input: `(id, nonce, grant, exp)`; the grant binds the
/// capability to the survey the attempt belongs to.
fn mac_input(id: &Uuid, nonce: &str, grant: &str, exp: i64) -> String {
    format!("{id}.{nonce}.{grant}.{exp}")
}

fn grant_for(survey_id: Uuid) -> String {
    format!("survey_input:{survey_id}")
}

fn compute_mac(secret: &[u8], input: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(input.as_bytes());
    mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

/// Constant-time MAC comparison.
fn verify_mac(secret: &[u8], input: &str, provided: &str) -> bool {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(input.as_bytes());
    match hex_decode(provided) {
        Some(bytes) => mac.verify_slice(&bytes).is_ok(),
        None => false,
    }
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    for pair in bytes.chunks(2) {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        out.push((hi * 16 + lo) as u8);
    }
    Some(out)
}

// ─── the Tier B failure book ──────────────────────────────────────────────────

/// One identity's failure state in the lockout book.
#[derive(Debug, Clone)]
struct FailureEntry {
    failures: i32,
    locked_until: Option<DateTime<Utc>>,
    last_attempt: Option<DateTime<Utc>>,
}

/// The in-memory lockout book: per-identity AND per-IP counters, keyed by
/// the caller (`code|id:{identity}` / `code|ip:{ip}`). Pure bookkeeping —
/// the escalation curve lives in the pure [`lockout_until`].
#[derive(Debug, Default)]
pub struct CodeFailureBook {
    entries: Mutex<HashMap<String, FailureEntry>>,
}

impl CodeFailureBook {
    pub fn new() -> Self {
        Self::default()
    }

    /// Is this key currently locked out (or spacing-gated)? Returns the
    /// typed refusal the caller should surface, if any.
    pub fn check(&self, key: &str, now: DateTime<Utc>) -> Result<(), SurveyWriteError> {
        let guard = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = guard.get(key) {
            if let Some(until) = entry.locked_until {
                if now < until {
                    return Err(SurveyWriteError::SessionCodeLocked {
                        retry_after_seconds: (until - now).num_seconds().max(1),
                    });
                }
            }
            if let Some(last) = entry.last_attempt {
                if now.signed_duration_since(last) < CODE_ATTEMPT_SPACING {
                    return Err(SurveyWriteError::SessionCodeSpacing);
                }
            }
        }
        Ok(())
    }

    /// Register a failure: increment, recompute the lockout, stamp the
    /// attempt time.
    pub fn register_failure(&self, key: &str, now: DateTime<Utc>) {
        let mut guard = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let entry = guard.entry(key.to_string()).or_insert(FailureEntry {
            failures: 0,
            locked_until: None,
            last_attempt: None,
        });
        entry.failures += 1;
        entry.locked_until = lockout_until(entry.failures, now);
        entry.last_attempt = Some(now);
    }

    /// Success resets the counter (the identity proved itself).
    pub fn reset(&self, key: &str) {
        let mut guard = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        guard.remove(key);
    }

    /// The recorded failure count (probe visibility).
    pub fn failures(&self, key: &str) -> i32 {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .map(|e| e.failures)
            .unwrap_or(0)
    }
}

// ─── the service ──────────────────────────────────────────────────────────────

/// One invited recipient.
#[derive(Debug, Clone)]
pub struct InviteRecipient {
    pub partner_id: Option<Uuid>,
    pub email: Option<String>,
    pub nickname: Option<String>,
    pub user_id: Option<Uuid>,
}

/// The minted attempt + its capability link (what every entry path
/// returns; the link is the ONLY thing that authorizes the taker).
#[derive(Debug)]
pub struct AttemptTicket {
    pub input: UserInput,
    pub link: String,
}

/// Split a free-form recipient blob on the upstream separators.
pub fn split_free_form_emails(blob: &str) -> Vec<String> {
    blob.split([';', ',', '\n', '\r'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// The entry/join/token service.
pub struct AttemptService {
    pool: PgPool,
    secret: Vec<u8>,
    sink: EventSinkSlot,
    failures: Arc<CodeFailureBook>,
}

impl AttemptService {
    /// Construct with the HMAC secret from the environment (empty = not
    /// configured; minting then fails loudly — no zero-secret fallback).
    pub fn new(pool: PgPool, sink: EventSinkSlot) -> Self {
        let secret = std::env::var(SURVEY_TOKEN_SECRET_ENV).unwrap_or_default();
        Self {
            pool,
            secret: secret.into_bytes(),
            sink,
            failures: Arc::new(CodeFailureBook::new()),
        }
    }

    /// Construct with an explicit secret (composition + probes).
    pub fn with_secret(pool: PgPool, sink: EventSinkSlot, secret: &[u8]) -> Self {
        Self { pool, secret: secret.to_vec(), sink, failures: Arc::new(CodeFailureBook::new()) }
    }

    pub fn failure_book(&self) -> Arc<CodeFailureBook> {
        self.failures.clone()
    }

    fn require_secret(&self) -> Result<&[u8], SurveyWriteError> {
        if self.secret.is_empty() {
            return Err(SurveyWriteError::SecretNotConfigured);
        }
        Ok(&self.secret)
    }

    // ── Tier A ────────────────────────────────────────────────────────────────

    /// Render the public capability link for an input.
    pub fn mint_capability(&self, input: &UserInput) -> Result<String, SurveyWriteError> {
        let secret = self.require_secret()?;
        let exp = input.token_expires_at.timestamp();
        let input_str = mac_input(&input.id, &input.token_nonce, &grant_for(input.survey_id), exp);
        let mac = compute_mac(secret, &input_str);
        Ok(format!("{}.{}.{}.{}", input.id, input.token_nonce, exp, mac))
    }

    /// Full verification: parse → load → recompute the MAC over the
    /// STORED fields → expiry (typed 410) → terminal-state refusal.
    /// Unknown, malformed, forged, and finished share one refusal.
    pub async fn verify_capability(
        &self,
        link: &str,
    ) -> Result<UserInput, SurveyWriteError> {
        let secret = self.require_secret()?;
        let parts = parse_capability(link).ok_or(SurveyWriteError::AttemptNotSubmittable)?;

        let mut tx = self.pool.begin().await?;
        let row = AttemptRepository::find_live_by_id(&mut tx, parts.id)
            .await?
            .ok_or(SurveyWriteError::AttemptNotSubmittable)?;

        let expected_input =
            mac_input(&row.id, &row.token_nonce, &grant_for(row.survey_id), row.token_expires_at.timestamp());
        if !verify_mac(secret, &expected_input, &parts.mac)
            || parts.nonce != row.token_nonce
            || parts.exp != row.token_expires_at.timestamp()
        {
            tracing::warn!(input = %row.id, "survey_token_verify_failed");
            tx.commit().await?;
            return Err(SurveyWriteError::AttemptNotSubmittable);
        }
        tx.commit().await?;

        // Expiry: typed 410, same shared body as every other refusal.
        if Utc::now() > row.token_expires_at {
            return Err(SurveyWriteError::AttemptExpired);
        }
        // Terminal: the capability multi-use window closed with the
        // attempt.
        if row.state == SurveyInputState::Done {
            return Err(SurveyWriteError::AttemptNotSubmittable);
        }
        Ok(row)
    }

    /// The guarded rotation verb: fresh nonce + expiry, the old nonce
    /// dies atomically. Refused on terminal/missing rows.
    pub async fn rotate_token(
        &self,
        input_id: Uuid,
        ttl_days: Option<i64>,
    ) -> Result<AttemptTicket, SurveyWriteError> {
        let _ = self.require_secret()?;
        let mut tx = self.pool.begin().await?;
        let nonce = mint_nonce();
        let expires = Utc::now() + Duration::days(ttl_days.unwrap_or(DEFAULT_TOKEN_TTL_DAYS));
        if !AttemptRepository::rotate_nonce(&mut tx, input_id, &nonce, expires).await? {
            return Err(SurveyWriteError::AttemptNotSubmittable);
        }
        let row = AttemptRepository::find_live_by_id(&mut tx, input_id)
            .await?
            .ok_or(SurveyWriteError::InputNotFound(input_id))?;
        tx.commit().await?;
        let link = self.mint_capability(&row)?;
        Ok(AttemptTicket { input: row, link })
    }

    // ── entry paths ───────────────────────────────────────────────────────────

    /// The shared mint: pool gate → insert row (state per caller) →
    /// frozen denominator snapshot → capability link.
    #[allow(clippy::too_many_arguments)]
    async fn mint_attempt(
        &self,
        conn: &mut sqlx::PgConnection,
        survey: &Survey,
        invite_token: Option<&str>,
        partner_id: Option<Uuid>,
        email: Option<&str>,
        nickname: Option<&str>,
        user_id: Option<Uuid>,
        wire_identity_key: Option<&str>,
        test_entry: bool,
        state: SurveyInputState,
        is_session_answer: bool,
    ) -> Result<UserInput, SurveyWriteError> {
        let nonce = mint_nonce();
        let expires = Utc::now() + Duration::days(DEFAULT_TOKEN_TTL_DAYS);
        let deadline = if survey.is_time_limited {
            Some(Utc::now() + Duration::seconds((survey.time_limit * 60.0) as i64))
        } else {
            None
        };
        let input = AttemptRepository::insert_input(
            conn,
            Uuid::new_v4(),
            survey.id,
            &nonce,
            expires,
            invite_token,
            partner_id,
            email,
            nickname,
            user_id,
            wire_identity_key,
            test_entry,
            state,
            deadline,
            is_session_answer,
        )
        .await?;

        // THE frozen denominator: snapshot the live questions (the full
        // non-page set, in random order under random selection — the
        // schema carries no per-input count knob, so random selection
        // randomizes ORDER over the whole set) with their weights.
        let sampled: Option<Vec<Uuid>> = if survey
            .questions_selection
            == crate::domain::entity::SurveyQuestionsSelection::Random
        {
            let all = SurveySessionRepository::questions_in_sequence(conn, survey.id).await?;
            Some(random_sample(&all, all.len()))
        } else {
            None
        };
        ScoringRepository::snapshot_denominator(
            conn,
            input.id,
            survey.id,
            sampled.as_deref(),
        )
        .await?;
        Ok(input)
    }

    /// The anti-cheat pool gate: `_has_attempts_left` against the live
    /// pool count (flushed inside the caller's transaction).
    async fn has_attempts_left(
        conn: &mut sqlx::PgConnection,
        survey: &Survey,
        invite_token: Option<&str>,
        partner_id: Option<Uuid>,
        email: Option<&str>,
    ) -> Result<bool, SurveyWriteError> {
        if !survey.is_attempts_limited {
            return Ok(true);
        }
        let used = AttemptRepository::pool_count(
            conn,
            survey.id,
            invite_token,
            partner_id,
            email,
            None,
            false,
        )
        .await?;
        Ok(used < survey.attempts_limit.max(0) as i64)
    }

    /// Public (non-session) entry via the survey's URL key: intake gate
    /// (active, public access mode, attempts left, no login requirement)
    /// → attempt pre-creation (THE anti-cheat: the row exists before any
    /// submit) + Tier A mint.
    pub async fn public_start(
        &self,
        survey_token: &str,
        email: Option<&str>,
        nickname: Option<&str>,
        user_id: Option<Uuid>,
    ) -> Result<AttemptTicket, SurveyWriteError> {
        let _ = self.require_secret()?;
        let mut tx = self.pool.begin().await?;
        let survey = SurveySessionRepository::find_survey_by_access_token(&mut tx, survey_token)
            .await?
            .ok_or(SurveyWriteError::SurveyNotPublicAccess)?;
        if !survey.active {
            return Err(SurveyWriteError::SurveyClosed);
        }
        if survey.access_mode != crate::domain::entity::SurveyAccessMode::Public {
            return Err(SurveyWriteError::SurveyNotPublicAccess);
        }
        if survey.users_login_required && user_id.is_none() {
            return Err(SurveyWriteError::LoginRequired);
        }
        if !Self::has_attempts_left(&mut tx, &survey, None, None, email).await? {
            return Err(SurveyWriteError::AttemptsExhausted { limit: survey.attempts_limit });
        }
        let input = self
            .mint_attempt(
                &mut tx,
                &survey,
                None,
                None,
                email,
                nickname,
                user_id,
                None,
                false,
                SurveyInputState::New,
                false,
            )
            .await?;
        tx.commit().await?;
        let link = self.mint_capability(&input)?;
        Ok(AttemptTicket { input, link })
    }

    /// Tier B verify + join: lockout gates → the code must be live
    /// (state-based + TTL) → attempt pre-creation (session answer) +
    /// capability mint + the host handle stamped for the realtime
    /// resolver.
    pub async fn join_by_code(
        &self,
        code: &str,
        identity: &str,
        ip: &str,
        wire_identity_key: &str,
        nickname: Option<&str>,
    ) -> Result<AttemptTicket, SurveyWriteError> {
        let _ = self.require_secret()?;
        let now = Utc::now();
        let id_key = format!("code:{code}|id:{identity}");
        let ip_key = format!("code:{code}|ip:{ip}");
        self.failures.check(&id_key, now)?;
        self.failures.check(&ip_key, now)?;

        let mut tx = self.pool.begin().await?;
        let survey = SurveySessionRepository::find_survey_by_session_code(&mut tx, code)
            .await?;
        let armed_at = match &survey {
            Some(s) => session_armed_at(&mut tx, s.id).await?,
            None => None,
        };
        // Wrong code and dead code answer the SAME refusal (no oracle).
        let live = survey
            .as_ref()
            .map(|s| session_code_live(s, armed_at, now))
            .unwrap_or(false);
        if !live {
            tx.commit().await?;
            self.failures.register_failure(&id_key, now);
            self.failures.register_failure(&ip_key, now);
            tracing::warn!(code_shape_ok = !code.is_empty(), "survey_code_verify_failed");
            return Err(SurveyWriteError::SessionCodeNotValid);
        }
        let survey = survey.expect("live implies Some");

        // Session attendees may enter directly in_progress when the
        // session already runs (T0); a `ready` session still admits as
        // `new` (the begin verb opens the attempt).
        let state = if survey.session_state
            == Some(crate::domain::entity::SurveySessionState::InProgress)
        {
            SurveyInputState::InProgress
        } else {
            SurveyInputState::New
        };
        let input = self
            .mint_attempt(
                &mut tx,
                &survey,
                None,
                None,
                None,
                nickname,
                None,
                Some(wire_identity_key),
                false,
                state,
                true,
            )
            .await?;
        if state == SurveyInputState::InProgress && input.start_datetime.is_none() {
            // Direct-entry attendees still carry a start stamp.
            sqlx::query(
                r#"UPDATE survey.survey_user_inputs SET start_datetime = $2 WHERE id = $1"#,
            )
            .bind(input.id)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;

        // Success resets BOTH books (identity + IP).
        self.failures.reset(&id_key);
        self.failures.reset(&ip_key);
        let link = self.mint_capability(&input)?;
        Ok(AttemptTicket { input, link })
    }

    /// Test-mode entry (`test_entry = true` — excluded from attempt
    /// counts AND KPI computes).
    pub async fn test_start(&self, survey_id: Uuid) -> Result<AttemptTicket, SurveyWriteError> {
        let _ = self.require_secret()?;
        let mut tx = self.pool.begin().await?;
        let survey = SurveySessionRepository::find_survey_by_id(&mut tx, survey_id)
            .await?
            .ok_or(SurveyWriteError::SurveyNotFound(survey_id))?;
        if !survey.active {
            return Err(SurveyWriteError::SurveyClosed);
        }
        let input = self
            .mint_attempt(
                &mut tx,
                &survey,
                None,
                None,
                None,
                Some("test"),
                None,
                None,
                true,
                SurveyInputState::New,
                false,
            )
            .await?;
        tx.commit().await?;
        let link = self.mint_capability(&input)?;
        Ok(AttemptTicket { input, link })
    }

    /// The invitation batch: recipients + free-form emails (split,
    /// validated, deduped), `existing_mode` new/resend, deadline
    /// propagation (mint stamps it from the survey's time limit), one
    /// attempt + Tier A link per recipient, one `participant_invited`
    /// fact per recipient. Failures are per-recipient (isolation), the
    /// batch continues.
    pub async fn invite(
        &self,
        survey_id: Uuid,
        recipients: Vec<InviteRecipient>,
        free_form_emails: &str,
        deadline: Option<DateTime<Utc>>,
        existing_mode: SurveyInviteExistingMode,
    ) -> Result<Vec<Result<AttemptTicket, SurveyWriteError>>, SurveyWriteError> {
        let _ = self.require_secret()?;
        let mut tx = self.pool.begin().await?;
        let survey = SurveySessionRepository::find_survey_by_id(&mut tx, survey_id)
            .await?
            .ok_or(SurveyWriteError::SurveyNotFound(survey_id))?;
        if !survey.active {
            return Err(SurveyWriteError::SurveyClosed);
        }

        // Free-form blob → validated, deduped recipients.
        let mut all = recipients;
        let mut seen: std::collections::HashSet<String> =
            all.iter().filter_map(|r| r.email.clone()).collect();
        for email in split_free_form_emails(free_form_emails) {
            if !email.contains('@') {
                continue; // validated: skip non-addresses loudly recorded
            }
            if seen.insert(email.clone()) {
                all.push(InviteRecipient {
                    partner_id: None,
                    email: Some(email),
                    nickname: None,
                    user_id: None,
                });
            }
        }

        let invite_token = Uuid::new_v4().to_string();
        let mut out = Vec::with_capacity(all.len());
        for r in &all {
            // Resend mode: a recipient with a live open attempt gets a
            // FRESH capability on the SAME row (rotation), not a second
            // attempt in the pool.
            if existing_mode == SurveyInviteExistingMode::Resend {
                let existing = sqlx::query_as::<_, UserInput>(
                    r#"SELECT * FROM survey.survey_user_inputs
                       WHERE survey_id = $1 AND state <> 'done'
                         AND (metadata->>'deleted_at') IS NULL
                         AND (email = $2 OR partner_id = $3)
                       ORDER BY (metadata->>'created_at') DESC NULLS LAST LIMIT 1"#,
                )
                .bind(survey_id)
                .bind(&r.email)
                .bind(r.partner_id)
                .fetch_optional(&mut *tx)
                .await?;
                if let Some(prior) = existing {
                    let nonce = mint_nonce();
                    let expires =
                        Utc::now() + Duration::days(DEFAULT_TOKEN_TTL_DAYS);
                    AttemptRepository::rotate_nonce(&mut tx, prior.id, &nonce, expires).await?;
                    let row = AttemptRepository::find_live_by_id(&mut tx, prior.id)
                        .await?
                        .ok_or(SurveyWriteError::InputNotFound(prior.id))?;
                    let link = self.mint_capability(&row)?;
                    self.sink.record(&Fact::ParticipantInvited {
                        survey_id,
                        input_id: row.id,
                        email: r.email.clone(),
                        link: link.clone(),
                    });
                    out.push(Ok(AttemptTicket { input: row, link }));
                    continue;
                }
            }
            let result = self
                .mint_attempt(
                    &mut tx,
                    &survey,
                    Some(&invite_token),
                    r.partner_id,
                    r.email.as_deref(),
                    r.nickname.as_deref(),
                    r.user_id,
                    None,
                    false,
                    SurveyInputState::New,
                    false,
                )
                .await;
            match result {
                Ok(input) => {
                    if let Some(dl) = deadline {
                        sqlx::query(
                            r#"UPDATE survey.survey_user_inputs SET deadline = $2 WHERE id = $1"#,
                        )
                        .bind(input.id)
                        .bind(dl)
                        .execute(&mut *tx)
                        .await?;
                    }
                    let link = self.mint_capability(&input)?;
                    self.sink.record(&Fact::ParticipantInvited {
                        survey_id,
                        input_id: input.id,
                        email: r.email.clone(),
                        link: link.clone(),
                    });
                    out.push(Ok(AttemptTicket { input, link }));
                }
                Err(e) => out.push(Err(e)),
            }
        }
        tx.commit().await?;
        Ok(out)
    }

    /// Pool re-entry (resend): a FRESH attempt under the SAME invite
    /// token (the pool's identity leg).
    pub async fn resend(
        &self,
        input_id: Uuid,
    ) -> Result<AttemptTicket, SurveyWriteError> {
        let _ = self.require_secret()?;
        let mut tx = self.pool.begin().await?;
        let prior = AttemptRepository::find_live_by_id(&mut tx, input_id)
            .await?
            .ok_or(SurveyWriteError::InputNotFound(input_id))?;
        let survey = SurveySessionRepository::find_survey_by_id(&mut tx, prior.survey_id)
            .await?
            .ok_or(SurveyWriteError::SurveyNotFound(prior.survey_id))?;
        if !survey.active {
            return Err(SurveyWriteError::SurveyClosed);
        }
        if !Self::has_attempts_left(
            &mut tx,
            &survey,
            prior.invite_token.as_deref(),
            prior.partner_id,
            prior.email.as_deref(),
        )
        .await?
        {
            return Err(SurveyWriteError::AttemptsExhausted { limit: survey.attempts_limit });
        }
        let input = self
            .mint_attempt(
                &mut tx,
                &survey,
                prior.invite_token.as_deref(),
                prior.partner_id,
                prior.email.as_deref(),
                prior.nickname.as_deref(),
                prior.user_id,
                prior.wire_identity_key.as_deref(),
                prior.test_entry,
                SurveyInputState::New,
                prior.is_session_answer,
            )
            .await?;
        tx.commit().await?;
        let link = self.mint_capability(&input)?;
        Ok(AttemptTicket { input, link })
    }
}

/// Uniform sample of `n` ids (Fisher-Yates over indexes).
fn random_sample(questions: &[crate::domain::entity::Question], n: usize) -> Vec<Uuid> {
    let mut idxs: Vec<usize> = (0..questions.len()).collect();
    let mut rng = rand::thread_rng();
    use rand::seq::SliceRandom;
    idxs.shuffle(&mut rng);
    idxs.into_iter().take(n).map(|i| questions[i].id).collect()
}
