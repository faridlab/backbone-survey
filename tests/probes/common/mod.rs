//! Shared harness: one DISPOSABLE scratch database per test, FAIL-HARD.
//!
//! The suite never runs against a shared DB: each test mints
//! `survey_seat_<marker>_<hex>` on the local test Postgres (5433,
//! postgres/postgres — the pinned scratch container), applies this
//! module's migrations with a raw SQL file runner, runs, and drops the
//! database.
//!
//! FAIL-HARD CONTRACT (copied from the mailing harness, tightened): a
//! test that cannot reach its scratch database PANICS —
//! [`TestDb::new`] refuses to return `None`, and [`skipped`] panics on
//! principle. A skipped probe is a FAILED probe: a green suite means the
//! behaviors were exercised, not that they were unreachable. Every
//! failure branch prints WHY before panicking.
//!
//! `TestDb::dispose()` is the explicit teardown; `Drop` is the leak guard
//! (best-effort DROP on a throwaway runtime) for tests that panic.

use std::sync::{Arc, Once};
use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

use backbone_survey::application::service::attempt_service::AttemptService;
use backbone_survey::application::service::certification_port::CertificationGrantSlot;
use backbone_survey::application::service::event_sink::{EventSinkSlot, RecordingEventSink};
use backbone_survey::application::service::intake_service::IntakeService;
use backbone_survey::application::service::scoring_service::ScoringService;
use backbone_survey::application::service::session_read_service::SessionReadService;
use backbone_survey::application::service::survey_write_service::SurveyWriteService;

/// The scratch Postgres every test database is born on and dropped from.
pub const SCRATCH_ADMIN_URL: &str = "postgres://postgres:postgres@localhost:5433/postgres";

/// The HMAC secret the probe services share (explicit, never from the
/// environment — the probes must not depend on host configuration).
pub const PROBE_SECRET: &[u8] = b"survey-probe-secret";

fn admin_url() -> String {
    std::env::var("SURVEY_TEST_ADMIN_URL").unwrap_or_else(|_| SCRATCH_ADMIN_URL.into())
}

/// The fail-hard skip: reaching this is a FAILURE, never a green tick.
pub fn skipped(reason: &str) -> ! {
    panic!("VACUOUS SKIP IS A FAILURE: {reason}");
}

/// The module builders that need an environment secret read it exactly
/// once per process; the probes pin it so `SurveyModule::builder()`
/// composition paths work too.
pub fn ensure_token_secret() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if std::env::var(backbone_survey::application::service::attempt_service::SURVEY_TOKEN_SECRET_ENV)
            .is_err()
        {
            std::env::set_var(
                backbone_survey::application::service::attempt_service::SURVEY_TOKEN_SECRET_ENV,
                "survey-probe-secret",
            );
        }
    });
}

/// One disposable scratch database, migrations applied. Panics (never
/// returns `None`) when the scratch Postgres is unreachable — see the
/// module docs for the fail-hard contract.
pub struct TestDb {
    pub pool: PgPool,
    name: String,
    admin: PgPool,
}

impl TestDb {
    pub async fn new(marker: &str) -> Self {
        let url = admin_url();
        let admin = match PgPoolOptions::new()
            .max_connections(4)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&url)
            .await
        {
            Ok(a) => a,
            Err(e) => {
                eprintln!("PROBE-FAIL: {marker}: admin connect to {url} failed: {e}");
                skipped(&format!("scratch Postgres unreachable: {e}"));
            }
        };
        let suffix: String = Uuid::new_v4().simple().to_string().chars().take(8).collect();
        let name = format!("survey_seat_{marker}_{suffix}");
        // Disposable by construction: a stale DB of the same name goes first.
        if let Err(e) =
            sqlx::query(&format!(r#"DROP DATABASE IF EXISTS "{name}" WITH (FORCE)"#)).execute(&admin).await
        {
            eprintln!("PROBE-FAIL: {marker}: pre-drop of {name} failed: {e}");
            skipped(&format!("scratch pre-drop failed: {e}"));
        }
        if let Err(e) = sqlx::query(&format!(r#"CREATE DATABASE "{name}""#)).execute(&admin).await {
            eprintln!("PROBE-FAIL: {marker}: create database {name} failed: {e}");
            skipped(&format!("scratch create failed: {e}"));
        }
        // Splice ONLY the trailing path segment.
        let db_url = match url.rfind('/') {
            Some(i) => format!("{}{}", &url[..=i], name),
            None => url.clone(),
        };
        let pool = match PgPoolOptions::new()
            .max_connections(8)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&db_url)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("PROBE-FAIL: {marker}: connect to {db_url} failed: {e}");
                skipped(&format!("scratch connect failed: {e}"));
            }
        };
        if let Err(what) = apply_module_migrations(&pool, marker).await {
            skipped(&what);
        }
        Self { pool, name, admin }
    }

    /// Explicit teardown: drop the scratch database entirely.
    pub async fn dispose(self) {
        self.drop_db().await;
    }

    async fn drop_db(&self) {
        // FORCE: connected test pool may still hold an idle session.
        let _ = sqlx::query(&format!(r#"DROP DATABASE IF EXISTS "{}" WITH (FORCE)"#, self.name))
            .execute(&self.admin)
            .await;
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        let name = self.name.clone();
        let url = admin_url();
        // Leak-guard teardown for panicking tests; dispose() is the happy path.
        std::thread::spawn(move || {
            if let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() {
                rt.block_on(async move {
                    if let Ok(admin) = sqlx::PgPool::connect(&url).await {
                        let _ = sqlx::query(&format!(
                            r#"DROP DATABASE IF EXISTS "{name}" WITH (FORCE)"#
                        ))
                        .execute(&admin)
                        .await;
                    }
                });
            }
        });
    }
}

/// Apply this module's migrations with a raw SQL file runner (sorted
/// `.up.sql` order — the module's files are self-contained).
async fn apply_module_migrations(pool: &PgPool, marker: &str) -> Result<(), String> {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let dir = format!("{manifest}/migrations");
    let mut files: Vec<std::path::PathBuf> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name().and_then(|n| n.to_str()).map(|n| n.ends_with(".up.sql")).unwrap_or(false)
            })
            .collect(),
        Err(e) => return Err(format!("PROBE-FAIL: {marker}: cannot read {dir}: {e}")),
    };
    files.sort();
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| format!("PROBE-FAIL: {marker}: cannot acquire pool conn: {e}"))?;
    for file in files {
        let sql = std::fs::read_to_string(&file)
            .map_err(|e| format!("PROBE-FAIL: {marker}: cannot read {}: {e}", file.display()))?;
        if let Err(e) = sqlx::raw_sql(&sql).execute(&mut *conn).await {
            return Err(format!("PROBE-FAIL: {marker}: migration {} failed: {e}", file.display()));
        }
    }
    Ok(())
}

// ── the service bundle (explicit secret; no environment dependence) ─────────

/// Every hand service over one pool, sharing one recording sink and one
/// certification slot the probe can install doubles into.
pub struct Svc {
    pub attempts: Arc<AttemptService>,
    pub intake: IntakeService,
    pub writes: SurveyWriteService,
    pub reads: SessionReadService,
    pub scoring: ScoringService,
    pub cert_slot: CertificationGrantSlot,
    pub sink: EventSinkSlot,
    /// The same recording double the slot was built with — the probe's
    /// fact-assertion window.
    pub recording: Arc<RecordingEventSink>,
}

impl Svc {
    pub fn new(pool: PgPool) -> Self {
        let recording = Arc::new(RecordingEventSink::default());
        let sink = EventSinkSlot::new(recording.clone());
        let cert_slot = CertificationGrantSlot::default();
        let attempts = Arc::new(AttemptService::with_secret(pool.clone(), sink.clone(), PROBE_SECRET));
        let intake = IntakeService::new(pool.clone(), attempts.clone(), sink.clone(), cert_slot.clone());
        let writes = SurveyWriteService::new(pool.clone(), sink.clone());
        let reads = SessionReadService::new(pool.clone());
        let scoring = ScoringService::new(pool);
        Self { attempts, intake, writes, reads, scoring, cert_slot, sink, recording }
    }

    /// The facts recorded so far, in order.
    pub fn facts(&self) -> Vec<backbone_survey::application::service::event_sink::Fact> {
        self.recording.recorded()
    }
}

// ── seeding helpers (direct SQL — tests may bypass the repositories) ────────

/// Insert a survey with sane defaults and return `(id, access_token)`.
/// `scoring_type` is `'no_scoring' | 'scoring_with_answers' |
/// 'scoring_without_answers' | 'scoring_with_answers_after_page'` (text,
/// implicitly cast to the enum).
#[allow(clippy::too_many_arguments)]
pub async fn seed_survey_opts(
    pool: &PgPool,
    scoring_type: &str,
    success_min: f64,
    time_limit_min: Option<f64>,
    attempts_limit: Option<i32>,
    can_go_back: bool,
    speed_rating: Option<i32>,
) -> (Uuid, String) {
    let id = Uuid::new_v4();
    let token = format!("tok_{}", Uuid::new_v4().simple());
    sqlx::query(
        r#"INSERT INTO survey.survey_surveys
             (id, title, access_token, scoring_type, scoring_success_min,
              is_time_limited, time_limit, is_attempts_limited, attempts_limit,
              users_can_go_back, session_speed_rating, session_speed_rating_time_limit)
           VALUES ($1, 'probe survey', $2, $3::survey_scoring_type, $4, $5, $6, $7, $8, $9, $10, $11)"#,
    )
    .bind(id)
    .bind(&token)
    .bind(scoring_type)
    .bind(success_min)
    .bind(time_limit_min.is_some())
    .bind(time_limit_min.unwrap_or(10.0))
    .bind(attempts_limit.is_some())
    .bind(attempts_limit.unwrap_or(1))
    .bind(can_go_back)
    .bind(speed_rating.is_some())
    .bind(speed_rating)
    .execute(pool)
    .await
    .expect("seed survey");
    (id, token)
}

/// The default survey: no scoring, no limits (the pure-runtime probes).
pub async fn seed_survey(pool: &PgPool) -> (Uuid, String) {
    seed_survey_opts(pool, "no_scoring", 80.0, None, None, false, None).await
}

/// A scored survey for the grading probes.
pub async fn seed_scored_survey(pool: &PgPool, success_min: f64) -> (Uuid, String) {
    seed_survey_opts(pool, "scoring_with_answers", success_min, None, None, false, None).await
}

/// A certification survey (login-gated, badge on, the clamp-forced
/// scoring type).
pub async fn seed_cert_survey(pool: &PgPool, success_min: f64) -> (Uuid, String) {
    let (id, token) =
        seed_survey_opts(pool, "scoring_without_answers", success_min, None, Some(2), false, None).await;
    sqlx::query(
        r#"UPDATE survey.survey_surveys
           SET certification = TRUE, users_login_required = TRUE,
               certification_give_badge = TRUE, certification_badge_key = 'probe-badge'
           WHERE id = $1"#,
    )
    .bind(id)
    .execute(pool)
    .await
    .expect("cert flags");
    (id, token)
}

/// Insert one typed question. `qtype` is the enum literal
/// ('numerical_box', 'simple_choice', ...). Returns its id.
pub async fn seed_question(
    pool: &PgPool,
    survey_id: Uuid,
    seq: i32,
    qtype: &str,
    weight: f64,
    scored: bool,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO survey.survey_questions
             (id, survey_id, sequence, title, question_type, answer_score,
              is_scored_question, answer_numerical_box)
           VALUES ($1, $2, $3, 'probe question', $4::survey_question_type, $5, $6, $7)"#,
    )
    .bind(id)
    .bind(survey_id)
    .bind(seq)
    .bind(qtype)
    .bind(weight)
    .bind(scored)
    .bind(if qtype == "numerical_box" { Some(0.0) } else { None })
    .execute(pool)
    .await
    .expect("seed question");
    id
}

/// Insert one choice/matrix-column label. Returns its id.
pub async fn seed_label(
    pool: &PgPool,
    question_id: Uuid,
    value: &str,
    is_correct: bool,
    score: f64,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO survey.survey_question_answers
             (id, question_id, value, is_correct, answer_score)
           VALUES ($1, $2, $3, $4, $5)"#,
    )
    .bind(id)
    .bind(question_id)
    .bind(value)
    .bind(is_correct)
    .bind(score)
    .execute(pool)
    .await
    .expect("seed label");
    id
}

/// Wire one conditional edge: `conditional_q` shows when `trigger_label`
/// (a label of the parent question) is chosen.
pub async fn seed_trigger(pool: &PgPool, conditional_q: Uuid, trigger_label: Uuid) {
    sqlx::query(
        r#"INSERT INTO survey.survey_question_triggering_answers (question_id, suggested_answer_id)
           VALUES ($1, $2)"#,
    )
    .bind(conditional_q)
    .bind(trigger_label)
    .execute(pool)
    .await
    .expect("seed trigger");
}

/// Insert a raw attempt row (direct SQL — the pre-creation/monotonic
/// probes need rows the service verbs did not mint). Returns its id.
pub async fn seed_input(pool: &PgPool, survey_id: Uuid, nonce: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO survey.survey_user_inputs (id, survey_id, token_nonce, token_expires_at)
           VALUES ($1, $2, $3, now() + interval '30 days')"#,
    )
    .bind(id)
    .bind(survey_id)
    .bind(nonce)
    .execute(pool)
    .await
    .expect("seed input");
    id
}

/// A ticket through the real public entry path (gate + mint + snapshot).
pub async fn start_attempt(svc: &Svc, token: &str) -> String {
    let ticket = svc
        .attempts
        .public_start(token, Some("probe@example.com"), Some("probe"), None)
        .await
        .expect("public start");
    ticket.link
}

/// The stored input row of a capability link (probe-side parse).
pub async fn input_of(pool: &PgPool, link: &str) -> backbone_survey::domain::entity::UserInput {
    let id = Uuid::parse_str(link.split('.').next().expect("link id segment")).expect("uuid");
    input_by_id(pool, id).await
}

/// The stored input row by id.
pub async fn input_by_id(pool: &PgPool, id: Uuid) -> backbone_survey::domain::entity::UserInput {
    sqlx::query_as::<_, backbone_survey::domain::entity::UserInput>(
        r#"SELECT * FROM survey.survey_user_inputs WHERE id = $1"#,
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .expect("input row")
}

/// The input's state as the DATABASE spells it (the entity field is
/// crate-private by design — the DB is the witness, not the struct).
pub async fn state_of(pool: &PgPool, id: Uuid) -> String {
    sqlx::query_scalar::<_, String>(r#"SELECT state::text FROM survey.survey_user_inputs WHERE id = $1"#)
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("state row")
}

/// Forge a link: same shape, wrong MAC secret leg.
pub fn forge_link(id: Uuid, nonce: &str, exp: i64) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = Hmac::<Sha256>::new_from_slice(b"wrong-secret").expect("hmac");
    mac.update(format!("{id}.{nonce}.forged-grant.{exp}").as_bytes());
    let mac_hex: String = mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect();
    format!("{id}.{nonce}.{exp}.{mac_hex}")
}
