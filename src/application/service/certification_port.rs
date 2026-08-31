//! The CertificationGrantPort — survey's seam to the engagement module's
//! badge-granting surface (hand-written; user-owned; see
//! `metaphor.codegen.yaml`).
//!
//! A certification survey's completion must publish the CertificationPassed
//! fact to engagement, which maps it onto `grant_system(kind=event,
//! grant_key="event:certification:{certification_ref}:{attempt_ref}")` —
//! exactly-once under at-least-once delivery. Engagement's badge masters
//! live behind that contract; this module takes NO code dependency on it
//! (no Cargo edge, no cross-schema FK), so the publish crosses as a PORT:
//! this module owns the trait, the host service registers ONE
//! implementation that emits onto its outbox with the pinned envelope.
//!
//! Deny-by-default (the PhoneBookPort / TraceClickPort shape): until a host
//! registers an implementation via `SurveyModule::set_certification_grant`,
//! every call fails with [`CertificationGrantError::NotComposed`] — a
//! missing composition is a LOUD refusal recorded as the audited critical
//! event `survey_certification_grant_refused`, never a silent no-grant that
//! would read as "user not certified".
//!
//! Producer-side once-per-user (SPEC: the publication gate publishes only
//! on the FIRST successful completion in the attempt pool — the attempt
//! pool self-join answers "first"; the idempotency key answers "exactly
//! once" on the consumer). The gate also refuses loudly when the winning
//! input carries no `user_id` (an anonymous attempt cannot earn a badge)
//! or when `certification_give_badge` is off — those refusals happen in
//! the write path BEFORE the port is called; the port is the delivery leg
//! only.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

/// Why a certification publish failed.
#[derive(Debug, Clone, thiserror::Error)]
pub enum CertificationGrantError {
    /// No [`CertificationGrantPort`] has been composed for this module —
    /// the deny-by-default refusal. Installing one via
    /// `SurveyModule::set_certification_grant` is the only cure. The caller
    /// records this as the audited critical event
    /// `survey_certification_grant_refused` (post-commit, per-input error
    /// isolation: the completion itself stands).
    #[error("certification grant port not composed: {detail}")]
    NotComposed { detail: String },
    /// The host-side delivery failed (outbox trouble, engagement refused).
    /// Retryable by the host's own outbox discipline; surfaced to the
    /// caller's audit record with the cause.
    #[error("certification grant delivery failed: {0}")]
    Delivery(String),
}

/// The completion fact, mirroring engagement's pinned `CertificationPassed`
/// payload field-for-field (backbone-engagement
/// `schema/hooks/index.hook.yaml`, event contract v1):
///
/// - `certification_ref` — `"survey:{survey_id}"`, the certification's own
///   logical id (the occurrence's parent).
/// - `survey_ref` — the issuing survey's uuid.
/// - `attempt_ref` — the WINNING input's uuid rendered as a string; with
///   `certification_ref` it forms the consumer's idempotency grant_key.
/// - `recipient_user_id` — the certified user (never NULL at this point:
///   the publication gate already refused anonymous winners).
/// - `badge_key` — the stable badge key from
///   `Survey.certification_badge_key`, resolved to a badge by the consumer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificationFact {
    pub certification_ref: String,
    pub survey_ref: Option<Uuid>,
    pub attempt_ref: String,
    pub recipient_user_id: Uuid,
    pub badge_key: String,
}

impl CertificationFact {
    /// Build the fact from the winning attempt's parts. `certification_ref`
    /// is always the `"survey:{id}"` form; `attempt_ref` is the winning
    /// input's uuid as a string.
    pub fn new(survey_id: Uuid, winning_input_id: Uuid, recipient_user_id: Uuid, badge_key: impl Into<String>) -> Self {
        Self {
            certification_ref: format!("survey:{survey_id}"),
            survey_ref: Some(survey_id),
            attempt_ref: winning_input_id.to_string(),
            recipient_user_id,
            badge_key: badge_key.into(),
        }
    }
}

/// The engagement seam. ONE method by design: the publish is fire-and-forget
/// at the call site (the caller flushes it post-commit with per-input error
/// isolation); delivery retries belong to the host's outbox, and
/// exactly-once belongs to the consumer's grant_key — this side is the
/// at-least-once producer.
#[async_trait]
pub trait CertificationGrantPort: Send + Sync {
    /// Deliver the completion fact toward engagement's badge grant. Err
    /// [`CertificationGrantError::NotComposed`] only when no host
    /// implementation is installed; transport/ downstream failures are
    /// [`CertificationGrantError::Delivery`] with the cause.
    async fn certification_passed(&self, fact: &CertificationFact) -> Result<(), CertificationGrantError>;
}

/// The deny-by-default implementation: the slot's initial tenant. Every
/// call refuses with [`CertificationGrantError::NotComposed`] — a missing
/// composition is a loud, audited refusal, never a silent no-grant.
pub struct RefusingCertificationGrant;

#[async_trait]
impl CertificationGrantPort for RefusingCertificationGrant {
    async fn certification_passed(&self, fact: &CertificationFact) -> Result<(), CertificationGrantError> {
        Err(CertificationGrantError::NotComposed {
            detail: format!(
                "no CertificationGrantPort is installed; refusing certification publish for \
                 attempt {} (badge key {}) — compose one via \
                 SurveyModule::set_certification_grant",
                fact.attempt_ref, fact.badge_key
            ),
        })
    }
}

/// The test double: records every fact it is handed and answers the
/// configured result. Exercises the happy path, the Delivery failure, and
/// (by leaving the slot uninstalled) the NotComposed refusal.
pub struct CannedCertificationGrant {
    pub deliveries: std::sync::Mutex<Vec<CertificationFact>>,
    pub result: Result<(), CertificationGrantError>,
}

impl CannedCertificationGrant {
    /// A double that accepts every fact.
    pub fn accepting() -> Self {
        Self { deliveries: std::sync::Mutex::new(Vec::new()), result: Ok(()) }
    }

    /// A double that refuses delivery with the given cause.
    pub fn failing(cause: impl Into<String>) -> Self {
        Self { deliveries: std::sync::Mutex::new(Vec::new()), result: Err(CertificationGrantError::Delivery(cause.into())) }
    }

    /// The facts handed to the double, in order.
    pub fn delivered(&self) -> Vec<CertificationFact> {
        self.deliveries.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

#[async_trait]
impl CertificationGrantPort for CannedCertificationGrant {
    async fn certification_passed(&self, fact: &CertificationFact) -> Result<(), CertificationGrantError> {
        self.deliveries.lock().unwrap_or_else(|e| e.into_inner()).push(fact.clone());
        self.result.clone()
    }
}

/// A shared, swappable grant-port slot — how a host registers its
/// composition without the generated module builder needing a new field.
/// The write path is built over the slot (defaulting to
/// [`RefusingCertificationGrant`]); `SurveyModule::set_certification_grant`
/// installs the host's implementation and every caller sees it on the NEXT
/// call. Reads are cheap (an `RwLock` read); installs happen once at boot.
#[derive(Clone)]
pub struct CertificationGrantSlot {
    inner: Arc<std::sync::RwLock<Arc<dyn CertificationGrantPort>>>,
}

impl CertificationGrantSlot {
    /// Install (replace) the active port.
    pub fn install(&self, port: Arc<dyn CertificationGrantPort>) {
        // A poisoned lock still holds the old value — recovering it beats
        // panicking every future call over one panicked writer.
        *self.inner.write().unwrap_or_else(|e| e.into_inner()) = port;
    }

    /// The currently installed port.
    pub fn current(&self) -> Arc<dyn CertificationGrantPort> {
        self.inner.read().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Default for CertificationGrantSlot {
    fn default() -> Self {
        Self { inner: Arc::new(std::sync::RwLock::new(Arc::new(RefusingCertificationGrant))) }
    }
}

#[async_trait]
impl CertificationGrantPort for CertificationGrantSlot {
    async fn certification_passed(&self, fact: &CertificationFact) -> Result<(), CertificationGrantError> {
        self.current().certification_passed(fact).await
    }
}
