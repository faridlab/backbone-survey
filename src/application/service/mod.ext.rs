// This directory's extension (hand-written; ADR-0031). The generated mod.rs (lib.rs for
// src/) pulls it in with include!, so its lines resolve against this directory.

pub mod certification_port;
pub mod event_sink;
pub mod attempt_service;
pub mod intake_service;
pub mod scoring_service;
pub mod session_read_service;
pub mod survey_write_service;
pub use certification_port::{
    CannedCertificationGrant, CertificationFact, CertificationGrantError, CertificationGrantPort,
    CertificationGrantSlot, RefusingCertificationGrant,
};
pub use event_sink::{
    EventSinkSlot, Fact, RecordingEventSink, SharedSurveyEventSink, SurveyEventSink,
    TracingEventSink,
};
pub use attempt_service::{
    lockout_until, AttemptService, CodeFailureBook, DEFAULT_TOKEN_TTL_DAYS,
    SURVEY_TOKEN_SECRET_ENV,
};
pub use intake_service::{AnswerDraft, IntakeService, SubmitOutcome};
pub use scoring_service::{speed_factor, AnswerValue, ScoringService};
pub use session_read_service::{parse_record_channel, record_channel, SessionReadService};
pub use survey_write_service::{
    apply_question_clamps, apply_survey_clamps, recompute_page_ids, session_code_live,
    SurveyWriteError, SurveyWriteService,
};
