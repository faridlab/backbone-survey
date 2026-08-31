//! Probes 11–12: the certification event contract — producer-side
//! exactly-once, and the fail-closed port (fail-hard; fresh scratch DB
//! per test).

use std::sync::Arc;

use uuid::Uuid;

use backbone_survey::application::service::certification_port::{
    CertificationFact, CertificationGrantError, CertificationGrantPort, CannedCertificationGrant,
};
use backbone_survey::application::service::event_sink::Fact;
use backbone_survey::application::service::intake_service::AnswerDraft;

use super::common::*;

/// Drive one user through a cert survey to a PASS: one correct answer
/// on a single weight-10 question = 100% (threshold 50).
async fn pass_cert(
    svc: &Svc,
    token: &str,
    email: &str,
    user_id: Uuid,
    question: Uuid,
) -> uuid::Uuid {
    let ticket = svc
        .attempts
        .public_start(token, Some(email), None, Some(user_id))
        .await
        .expect("cert start");
    svc.intake.begin(&ticket.link).await.expect("begin");
    svc.intake
        .submit_answer(&ticket.link, question, AnswerDraft::Number(0.0))
        .await
        .expect("correct answer");
    let final_row = svc.intake.finish(&ticket.link).await.expect("finish");
    assert!(final_row.scoring_success, "the attempt passed");
    final_row.id
}

/// Producer-side exactly-once: of two successful completions in one
/// attempt pool, only the FIRST publishes the CertificationPassed fact —
/// the pool gate (prior successes) answers "already certified".
#[tokio::test]
async fn p11_certification_exactly_once() {
    let db = TestDb::new("cert_once").await;
    let svc = Svc::new(db.pool.clone());
    let (survey_id, token) = seed_cert_survey(&db.pool, 50.0).await;
    let q1 = seed_question(&db.pool, survey_id, 10, "numerical_box", 10.0, true).await;

    let grant = Arc::new(CannedCertificationGrant::accepting());
    svc.cert_slot.install(grant.clone());

    let user = Uuid::new_v4();
    let first = pass_cert(&svc, &token, "cert-once@example.com", user, q1).await;

    // The fact crossed the port exactly once, field-for-field.
    let delivered = grant.delivered();
    assert_eq!(delivered.len(), 1, "the first success published exactly one fact");
    let fact = &delivered[0];
    assert_eq!(fact.certification_ref, format!("survey:{survey_id}"));
    assert_eq!(fact.survey_ref, Some(survey_id));
    assert_eq!(fact.attempt_ref, first.to_string(), "the WINNING attempt is the ref");
    assert_eq!(fact.recipient_user_id, user);
    assert_eq!(fact.badge_key, "probe-badge");

    // A SECOND successful completion in the same pool: no second publish.
    let second = pass_cert(&svc, &token, "cert-once@example.com", user, q1).await;
    assert_ne!(first, second);
    assert_eq!(
        grant.delivered().len(),
        1,
        "the pool gate suppresses the repeat publication"
    );

    // The completion fact itself fired for BOTH (it is per-input, the
    // certification gate is per-pool).
    let facts = svc.facts();
    let completions = facts
        .iter()
        .filter(|f| matches!(f, Fact::AnswerCompleted { input_id, .. } if *input_id == first || *input_id == second))
        .count();
    assert_eq!(completions, 2, "both completions crossed the sink");

    db.dispose().await;
}

/// The fail-closed port: deny-by-default (NotComposed refuses every
/// publish until a host composes one), a refused delivery NEVER rolls
/// the completion back, and the accepting composition delivers.
#[tokio::test]
async fn p12_certification_port_fail_closed() {
    let db = TestDb::new("cert_closed").await;
    let svc = Svc::new(db.pool.clone());
    let (survey_id, token) = seed_cert_survey(&db.pool, 50.0).await;
    let q1 = seed_question(&db.pool, survey_id, 10, "numerical_box", 10.0, true).await;

    // (a) Deny-by-default: the slot's initial tenant refuses loudly.
    let probe_fact = CertificationFact::new(survey_id, Uuid::new_v4(), Uuid::new_v4(), "probe");
    match svc.cert_slot.certification_passed(&probe_fact).await {
        Err(CertificationGrantError::NotComposed { .. }) => {}
        other => panic!("the uncomposed slot must refuse NotComposed, got {other:?}"),
    }

    // (b) A successful completion THROUGH the refusing default: the
    // completion stands (done + success + completion fact), no panic.
    let user_a = Uuid::new_v4();
    let a = pass_cert(&svc, &token, "user-a@example.com", user_a, q1).await;
    assert_eq!(state_of(&db.pool, a).await, "done", "the completion stands on refusal");
    assert!(
        svc.facts().iter().any(|f| matches!(f, Fact::AnswerCompleted { input_id, .. } if *input_id == a)),
        "the completion fact crossed the sink despite the refused publish"
    );

    // (c) A failing delivery (host-side trouble): the completion STILL
    // stands — the audit records the refusal, never a rollback.
    let failing = Arc::new(CannedCertificationGrant::failing("probe delivery refused"));
    svc.cert_slot.install(failing.clone());
    let user_b = Uuid::new_v4();
    let b = pass_cert(&svc, &token, "user-b@example.com", user_b, q1).await;
    assert_eq!(state_of(&db.pool, b).await, "done");
    assert_eq!(failing.delivered().len(), 1, "the delivery was attempted (and recorded)");

    // (d) The accepting composition delivers.
    let accepting = Arc::new(CannedCertificationGrant::accepting());
    svc.cert_slot.install(accepting.clone());
    let user_c = Uuid::new_v4();
    let c = pass_cert(&svc, &token, "user-c@example.com", user_c, q1).await;
    assert_eq!(state_of(&db.pool, c).await, "done");
    assert_eq!(accepting.delivered().len(), 1);
    assert_eq!(accepting.delivered()[0].recipient_user_id, user_c);

    db.dispose().await;
}
