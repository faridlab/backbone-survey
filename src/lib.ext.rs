//! The module's extension (hand-written; ADR-0031): what the module adds beside its
//! generated services. The generated module and builder carry `ModuleExt` and
//! `ModuleBuilderExt` and dereference to them, so their fields read as `module.field`.

#[allow(unused_imports)]
use super::*;

/// State the module adds; read through the generated module's `Deref`.
pub struct ModuleExt {
    /// The fail-closed certification seam (deny-by-default until the host
    /// installs one via `set_certification_grant`).
    pub(crate) certification_grant:
        crate::application::service::certification_port::CertificationGrantSlot,
    /// The shared event-sink seam (tracing-only until the host installs
    /// its relay via `set_event_sink`).
    pub(crate) event_sink:
        crate::application::service::event_sink::EventSinkSlot,
    pub(crate) attempt_service:
        std::sync::Arc<crate::application::service::AttemptService>,
    pub(crate) intake_service:
        std::sync::Arc<crate::application::service::IntakeService>,
    pub(crate) survey_write_service:
        std::sync::Arc<crate::application::service::SurveyWriteService>,
    pub(crate) session_read_service:
        std::sync::Arc<crate::application::service::SessionReadService>,
    pub(crate) scoring_service:
        std::sync::Arc<crate::application::service::ScoringService>,
    pub(crate) db_pool: sqlx::PgPool,
}

/// State the builder adds.
pub struct ModuleBuilderExt {
}

impl Default for ModuleBuilderExt {
    fn default() -> Self {
        Self {
        }
    }
}

impl ModuleBuilderExt {
    /// Build the extension's state from what the generated build made.
    #[allow(unused_variables, clippy::redundant_clone)]
    pub(crate) fn build(self, parts: &ModuleParts<'_>) -> anyhow::Result<ModuleExt> {
        let db_pool = parts.db_pool.clone();
        let certification_grant =
            crate::application::service::certification_port::CertificationGrantSlot::default();
        let event_sink = crate::application::service::event_sink::EventSinkSlot::default();
        let attempt_service = std::sync::Arc::new(
            crate::application::service::AttemptService::new(
                db_pool.clone(),
                event_sink.clone(),
            ),
        );
        let intake_service = std::sync::Arc::new(
            crate::application::service::IntakeService::new(
                db_pool.clone(),
                attempt_service.clone(),
                event_sink.clone(),
                certification_grant.clone(),
            ),
        );
        let survey_write_service = std::sync::Arc::new(
            crate::application::service::SurveyWriteService::new(
                db_pool.clone(),
                event_sink.clone(),
            ),
        );
        let session_read_service = std::sync::Arc::new(
            crate::application::service::SessionReadService::new(db_pool.clone()),
        );
        let scoring_service = std::sync::Arc::new(
            crate::application::service::ScoringService::new(db_pool.clone()),
        );
        Ok(ModuleExt {
            certification_grant,
            event_sink,
            attempt_service,
            intake_service,
            survey_write_service,
            session_read_service,
            scoring_service,
            db_pool,
        })
    }
}

impl crate::ThisModule {
    /// Compose the engagement badge-grant seam (deny-by-default until
    /// called — the PhoneBookPort shape): the ONE registered
    /// implementation delivers the CertificationPassed fact toward
    /// engagement's exactly-once grant. Until a host calls this, every
    /// certification publish refuses loudly with
    /// `CertificationGrantError::NotComposed` and the refusal is recorded
    /// as the audited critical event `survey_certification_grant_refused`.
    pub fn set_certification_grant(
        &self,
        port: std::sync::Arc<dyn crate::application::service::certification_port::CertificationGrantPort>,
    ) {
        self.certification_grant.install(port);
    }
    /// Compose the host event-sink relay (the mail-leg / realtime-relay
    /// seam). The default is tracing-only: a module built with no sink is
    /// observable, not silent.
    pub fn set_event_sink(
        &self,
        sink: std::sync::Arc<dyn crate::application::service::event_sink::SurveyEventSink>,
    ) {
        self.event_sink.install(sink);
    }
    pub fn attempt_service(&self) -> &crate::application::service::AttemptService {
        &self.attempt_service
    }
    pub fn intake_service(&self) -> &crate::application::service::IntakeService {
        &self.intake_service
    }
    pub fn survey_write_service(&self) -> &crate::application::service::SurveyWriteService {
        &self.survey_write_service
    }
    pub fn session_read_service(&self) -> &crate::application::service::SessionReadService {
        &self.session_read_service
    }
    pub fn scoring_service(&self) -> &crate::application::service::ScoringService {
        &self.scoring_service
    }
    /// The PUBLIC survey tree — BARE capability mounts (the token is the
    /// auth), throttled 120/min per client inside the composer. Mount at
    /// the host root.
    pub fn survey_public_routes(self: &Arc<Self>) -> Router {
        crate::presentation::http::public_routes::public_composer()
            .with_state(Arc::clone(self))
    }
    /// The SESSION start (`POST /survey/me/start/:survey_token`) — for the host's
    /// org-guarded tree: the taker is the session's principal (`OrgContext`), so a survey
    /// that requires a login can be taken. The public start cannot know who is taking.
    pub fn survey_session_routes(self: &Arc<Self>) -> Router {
        crate::presentation::http::public_routes::session_composer()
            .with_state(Arc::clone(self))
    }
    /// The GUARDED survey tree — the generated read-only base plus the
    /// officer verbs over the hand write services, for the host's
    /// authenticated tree (the host nests it at its `/api/v1/survey`
    /// behind identity + its module-write gate). Generic mutation never
    /// reaches the attempt tables.
    pub fn guarded_routes(self: &Arc<Self>) -> Router {
        self.readonly_routes().merge(
            crate::presentation::http::public_routes::guarded_composer()
                .with_state(Arc::clone(self)),
        )
    }
    /// The certification evidence view (the webapp certification page):
    /// the attempt's verdicts + scores, with stored VALUES revealed only
    /// under a with-answers scoring type. Requires a `scoring_success`
    /// input.
    pub async fn certification_evidence(
        &self,
        input_id: uuid::Uuid,
    ) -> Result<serde_json::Value, crate::application::service::SurveyWriteError> {
        use crate::application::service::SurveyWriteError as E;
        use crate::domain::entity::SurveyScoringType;
        let mut conn = crate::request_pool::begin(&self.db_pool).await?;
        let input =
            crate::infrastructure::persistence::AttemptRepository::find_live_by_id(
                &mut conn, input_id,
            )
            .await?
            .ok_or(E::InputNotFound(input_id))?;
        if !input.scoring_success {
            return Err(E::ValidationFailed {
                question_id: input_id,
                reason: "the attempt has not passed the scoring threshold".into(),
            });
        }
        let survey = crate::infrastructure::persistence::SurveySessionRepository::find_survey_by_id(
            &mut conn, input.survey_id,
        )
        .await?
        .ok_or(E::SurveyNotFound(input.survey_id))?;
        let reveal_values = matches!(
            survey.scoring_type,
            SurveyScoringType::ScoringWithAnswers | SurveyScoringType::ScoringWithAnswersAfterPage
        );
        let lines = sqlx::query_as::<_, (uuid::Uuid, bool, Option<f64>, Option<bool>, Option<String>, Option<String>)>(
            r#"SELECT l.question_id, l.skipped, l.answer_score, l.answer_is_correct,
                      l.value_char_box, l.value_text_box
               FROM survey.survey_user_input_lines l
               WHERE l.user_input_id = $1 AND (l.metadata->>'deleted_at') IS NULL"#,
        )
        .bind(input_id)
        .fetch_all(&mut *conn)
        .await?;
        let line_views: Vec<serde_json::Value> = lines
            .into_iter()
            .map(|(question_id, skipped, score, correct, ch, tx)| {
                let mut v = serde_json::json!({
                    "question_id": question_id,
                    "skipped": skipped,
                    "answer_score": score,
                    "answer_is_correct": correct,
                });
                if reveal_values {
                    v["value"] = serde_json::json!(ch.or(tx));
                }
                v
            })
            .collect();
        Ok(serde_json::json!({
            "input_id": input.id,
            "survey_id": survey.id,
            "scoring_percentage": input.scoring_percentage,
            "scoring_total": input.scoring_total,
            "scoring_success": input.scoring_success,
            "report_layout": survey.certification_report_layout.to_string(),
            "has_badge": survey.certification_badge_key.is_some(),
            "lines": line_views,
        }))
    }
    /// The leaderboard view: top-15 done session attendees by raw
    /// `scoring_total` with the CURRENT question's score subtracted (a
    /// question still live must not rank).
    pub async fn leaderboard_view(
        &self,
        survey_id: uuid::Uuid,
    ) -> Result<Vec<serde_json::Value>, crate::application::service::SurveyWriteError> {
        let snapshot = self.session_read_service().snapshot(survey_id).await?;
        let current = snapshot.current_question_id;
        let mut tx = crate::request_pool::begin(&self.db_pool).await?;
        let rows = sqlx::query_as::<_, (uuid::Uuid, Option<String>, f64, bool)>(
            r#"SELECT i.id, i.nickname,
                      i.scoring_total - COALESCE(cur.score, 0) AS total,
                      i.scoring_success
               FROM survey.survey_user_inputs i
               LEFT JOIN (
                   SELECT user_input_id, COALESCE(sum(answer_score), 0) AS score
                   FROM survey.survey_user_input_lines
                   WHERE survey_id = $1 AND question_id = $2
                   GROUP BY user_input_id
               ) cur ON cur.user_input_id = i.id
               WHERE i.survey_id = $1
                 AND i.is_session_answer
                 AND i.state = 'done'
                 AND i.test_entry IS NOT TRUE
                 AND (i.metadata->>'deleted_at') IS NULL
               ORDER BY total DESC, i.end_datetime ASC
               LIMIT 15"#,
        )
        .bind(survey_id)
        .bind(current)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(rows
            .into_iter()
            .map(|(id, nickname, total, success)| {
                serde_json::json!({
                    "input_id": id,
                    "nickname": nickname,
                    "total": total,
                    "success": success,
                })
            })
            .collect())
    }
    /// The statistics view: per-section (page) correct / partial /
    /// incorrect / skipped counts over done, non-test attempts.
    pub async fn statistics_view(
        &self,
        survey_id: uuid::Uuid,
    ) -> Result<Vec<serde_json::Value>, crate::application::service::SurveyWriteError> {
        let mut tx = crate::request_pool::begin(&self.db_pool).await?;
        let rows = sqlx::query_as::<_, (Option<uuid::Uuid>, i64, i64, i64, i64)>(
            r#"SELECT q.page_id,
                      count(*) FILTER (WHERE l.answer_is_correct) AS correct,
                      count(*) FILTER (WHERE l.answer_is_correct = false
                                          AND COALESCE(l.answer_score, 0) > 0) AS partial,
                      count(*) FILTER (WHERE l.answer_is_correct = false
                                          AND COALESCE(l.answer_score, 0) <= 0) AS incorrect,
                      count(*) FILTER (WHERE l.skipped) AS skipped
               FROM survey.survey_user_input_lines l
               JOIN survey.survey_user_inputs i ON i.id = l.user_input_id
               JOIN survey.survey_questions q ON q.id = l.question_id
               WHERE i.survey_id = $1
                 AND i.state = 'done'
                 AND i.test_entry IS NOT TRUE
                 AND (i.metadata->>'deleted_at') IS NULL
               GROUP BY q.page_id
               ORDER BY q.page_id NULLS FIRST"#,
        )
        .bind(survey_id)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(rows
            .into_iter()
            .map(|(page_id, correct, partial, incorrect, skipped)| {
                serde_json::json!({
                    "page_id": page_id,
                    "correct": correct,
                    "partial": partial,
                    "incorrect": incorrect,
                    "skipped": skipped,
                })
            })
            .collect())
    }
}
