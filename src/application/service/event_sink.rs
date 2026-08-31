//! The SurveyEventSink — survey's seam for host-relayed notification facts
//! (hand-written; user-owned; see `metaphor.codegen.yaml`).
//!
//! This module owns NO outbox schema (the host's `outbox_schemas` list is
//! untouched) and sends no mail, no push, no SMS itself. Everything
//! outbound crosses as FACTS on this sink; the host's implementation
//! stages them onto `messaging.outbox_events` under the record-shaped
//! realtime channel `survey.survey_{survey_id}` (ADR-0019 bare capability
//! mounts) or relays them to whatever notification path it composed.
//!
//! The default implementation ([`TracingEventSink`]) logs every fact — a
//! module built with no host sink is observable, not silent. Hosts that
//! want delivery register their implementation on the write service when
//! they compose it (the `with_event_sink` builder shape, the mailing
//! precedent).
//!
//! FACT VOCABULARY (one struct per fact, payload minimal + ids for
//! correlation):
//!  - [`Fact::ParticipantInvited`] — the invite verb minted an attempt;
//!    carries the per-attempt Tier A link payload for the host's mail leg.
//!  - [`Fact::AnswerCompleted`] — an attempt reached `done` (the funnel's
//!    terminal edge per input).
//!  - [`Fact::SessionStarted`] / [`Fact::SessionAdvanced`] /
//!    [`Fact::SessionEnded`] — the live-session cursor lifecycle that the
//!    realtime channel replays to attendees.

use std::sync::Arc;

use uuid::Uuid;

/// A host-relayed survey fact. Ids are the correlation keys; payloads are
/// the minimum the relay needs (rendered content is the webapp's concern).
#[derive(Debug, Clone, PartialEq)]
pub enum Fact {
    /// The invite verb minted an attempt for a participant. `link` is the
    /// READY-TO-SEND per-attempt URL — the Tier A capability rendered as
    /// `{input_id}.{nonce}.{exp}.{mac}` behind the public start route; the
    /// host mail leg embeds it verbatim and nothing else authorizes the
    /// taker.
    ParticipantInvited {
        survey_id: Uuid,
        input_id: Uuid,
        email: Option<String>,
        link: String,
    },
    /// An attempt reached `done` — the completion funnel's terminal edge
    /// for that input. (The certification leg is NOT this fact: it crosses
    /// the fail-closed [`crate::application::service::certification_port::CertificationGrantPort`].)
    AnswerCompleted {
        survey_id: Uuid,
        input_id: Uuid,
        scoring_percentage: f64,
        scoring_success: bool,
        test_entry: bool,
    },
    /// The session lazily opened on the first advance (`ready ->
    /// in_progress`); the leaderboard window's lower bound.
    SessionStarted {
        survey_id: Uuid,
        session_code: String,
    },
    /// The cursor moved — the realtime push's source event. `question_id`
    /// is the NEW current question; `payload_millis` is the PRE-write clock
    /// the attendee screens should render from (the deliberate
    /// attendee-favoring skew: the row carries now()+1s, the push carries
    /// the pre-write value).
    SessionAdvanced {
        survey_id: Uuid,
        session_code: String,
        question_id: Uuid,
        payload_millis: i64,
    },
    /// The end verb ran — attendees bulk-done forward-only, the code
    /// invalidated, the channel's closing fact.
    SessionEnded {
        survey_id: Uuid,
        session_code: String,
    },
}

impl Fact {
    /// The survey every fact is scoped to — the realtime channel's record
    /// id (`survey.survey_{survey_id}`).
    pub fn survey_id(&self) -> Uuid {
        match self {
            Fact::ParticipantInvited { survey_id, .. }
            | Fact::AnswerCompleted { survey_id, .. }
            | Fact::SessionStarted { survey_id, .. }
            | Fact::SessionAdvanced { survey_id, .. }
            | Fact::SessionEnded { survey_id, .. } => *survey_id,
        }
    }

    /// The tracing event name for the default sink (stable, greppable).
    pub fn name(&self) -> &'static str {
        match self {
            Fact::ParticipantInvited { .. } => "SurveyParticipantInvited",
            Fact::AnswerCompleted { .. } => "SurveyAnswerCompleted",
            Fact::SessionStarted { .. } => "SurveySessionStarted",
            Fact::SessionAdvanced { .. } => "SurveySessionAdvanced",
            Fact::SessionEnded { .. } => "SurveySessionEnded",
        }
    }
}

/// The relay seam. Synchronous by design: facts are cheap, ordered hints —
/// the host implementation stages them inside its own retrying discipline
/// and must never block the survey transaction on delivery.
pub trait SurveyEventSink: Send + Sync {
    /// Record one fact. Infallible on purpose: a relay problem is the
    /// host's observability story, never a survey-write rollback.
    fn record(&self, fact: &Fact);
}

/// The default sink: tracing only — a module built with no host sink is
/// observable, not silent.
#[derive(Debug, Default, Clone, Copy)]
pub struct TracingEventSink;

impl SurveyEventSink for TracingEventSink {
    fn record(&self, fact: &Fact) {
        match fact {
            Fact::ParticipantInvited { survey_id, input_id, email, link } => tracing::info!(
                survey_id = %survey_id,
                input_id = %input_id,
                email = ?email,
                has_link = !link.is_empty(),
                "SurveyParticipantInvited"
            ),
            Fact::AnswerCompleted { survey_id, input_id, scoring_percentage, scoring_success, test_entry } => {
                tracing::info!(
                    survey_id = %survey_id,
                    input_id = %input_id,
                    scoring_percentage,
                    scoring_success,
                    test_entry,
                    "SurveyAnswerCompleted"
                )
            }
            Fact::SessionStarted { survey_id, session_code } => tracing::info!(
                survey_id = %survey_id,
                session_code,
                "SurveySessionStarted"
            ),
            Fact::SessionAdvanced { survey_id, session_code, question_id, payload_millis } => {
                tracing::info!(
                    survey_id = %survey_id,
                    session_code,
                    question_id = %question_id,
                    payload_millis,
                    "SurveySessionAdvanced"
                )
            }
            Fact::SessionEnded { survey_id, session_code } => tracing::info!(
                survey_id = %survey_id,
                session_code,
                "SurveySessionEnded"
            ),
        }
    }
}

/// The recording test double: collects every fact for assertion.
#[derive(Debug, Default)]
pub struct RecordingEventSink {
    pub facts: std::sync::Mutex<Vec<Fact>>,
}

impl RecordingEventSink {
    /// The facts recorded so far, in order.
    pub fn recorded(&self) -> Vec<Fact> {
        self.facts.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl SurveyEventSink for RecordingEventSink {
    fn record(&self, fact: &Fact) {
        self.facts.lock().unwrap_or_else(|e| e.into_inner()).push(fact.clone());
    }
}

/// Shared sink handle — the write services hold one of these (defaulting
/// to [`TracingEventSink`]); the host overrides at composition time.
pub type SharedSurveyEventSink = Arc<dyn SurveyEventSink>;

/// A shared, swappable sink slot — the composition seam the write
/// services hold (the certification slot's shape). Defaults to
/// [`TracingEventSink`]; the host (or a probe) installs its relay via
/// [`EventSinkSlot::install`] and every caller sees it on the next fact.
#[derive(Clone)]
pub struct EventSinkSlot {
    inner: Arc<std::sync::RwLock<SharedSurveyEventSink>>,
}

impl EventSinkSlot {
    /// A slot holding the given sink.
    pub fn new(sink: SharedSurveyEventSink) -> Self {
        Self { inner: Arc::new(std::sync::RwLock::new(sink)) }
    }

    /// Install (replace) the active sink.
    pub fn install(&self, sink: SharedSurveyEventSink) {
        *self.inner.write().unwrap_or_else(|e| e.into_inner()) = sink;
    }

    /// The currently installed sink.
    pub fn current(&self) -> SharedSurveyEventSink {
        self.inner.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Record one fact through the currently installed sink.
    pub fn record(&self, fact: &Fact) {
        self.current().record(fact);
    }
}

impl Default for EventSinkSlot {
    fn default() -> Self {
        Self::new(Arc::new(TracingEventSink))
    }
}
