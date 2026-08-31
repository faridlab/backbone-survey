-- Hardening constraints for the survey schema: the DB-level guards the
-- schema DSL cannot express, plus the read-path indexes the attempt-pool
-- join and the leaderboard need.
--
-- CHECK constraints (named, on the live tables):
--   survey_surveys   scoring_success_min band; certification interlock;
--                    attempts/time-limit positivity; speed-rating interlock
--   survey_questions dual-nature XOR; answer-date typing; answer_score
--                    non-negativity; scale band; question time-limit
--                    positivity; validation range families
--   survey_question_answers  role XOR; value presence
-- Partial unique: the Tier A token_nonce among live rows.
-- Trigger: question DELETE block while the survey's session is in_progress
--          (a raw DELETE mid-session corrupts the running cursor).
-- Indexes: attempt-pool leading arm + leaderboard rank among done rows.

-- ── survey.survey_surveys ──────────────────────────────────────────────────
ALTER TABLE survey.survey_surveys DROP CONSTRAINT IF EXISTS chk_surveys_success_min_band;
ALTER TABLE survey.survey_surveys ADD CONSTRAINT chk_surveys_success_min_band
    CHECK (0 <= scoring_success_min AND scoring_success_min <= 100);

ALTER TABLE survey.survey_surveys DROP CONSTRAINT IF EXISTS chk_surveys_certification_scoring;
ALTER TABLE survey.survey_surveys ADD CONSTRAINT chk_surveys_certification_scoring
    CHECK (NOT certification OR scoring_type <> 'no_scoring');

ALTER TABLE survey.survey_surveys DROP CONSTRAINT IF EXISTS chk_surveys_attempts_limit_positive;
ALTER TABLE survey.survey_surveys ADD CONSTRAINT chk_surveys_attempts_limit_positive
    CHECK (NOT is_attempts_limited OR attempts_limit > 0);

ALTER TABLE survey.survey_surveys DROP CONSTRAINT IF EXISTS chk_surveys_time_limit_positive;
ALTER TABLE survey.survey_surveys ADD CONSTRAINT chk_surveys_time_limit_positive
    CHECK (NOT is_time_limited OR time_limit > 0);

ALTER TABLE survey.survey_surveys DROP CONSTRAINT IF EXISTS chk_surveys_speed_rating_limit;
ALTER TABLE survey.survey_surveys ADD CONSTRAINT chk_surveys_speed_rating_limit
    CHECK (NOT session_speed_rating OR session_speed_rating_time_limit > 0);

-- ── survey.survey_questions ────────────────────────────────────────────────
-- Dual-nature XOR: a row is either a page (is_page, NULL type) or a
-- question (typed). Upstream enforced this ORM-only; here it is a
-- constraint.
ALTER TABLE survey.survey_questions DROP CONSTRAINT IF EXISTS chk_questions_page_xor;
ALTER TABLE survey.survey_questions ADD CONSTRAINT chk_questions_page_xor
    CHECK (is_page = (question_type IS NULL));

-- Correct-date/datetime answers may only sit on scored questions of the
-- matching type.
ALTER TABLE survey.survey_questions DROP CONSTRAINT IF EXISTS chk_questions_answer_date_typed;
ALTER TABLE survey.survey_questions ADD CONSTRAINT chk_questions_answer_date_typed
    CHECK (answer_date IS NULL OR (is_scored_question AND question_type = 'date'));

ALTER TABLE survey.survey_questions DROP CONSTRAINT IF EXISTS chk_questions_answer_datetime_typed;
ALTER TABLE survey.survey_questions ADD CONSTRAINT chk_questions_answer_datetime_typed
    CHECK (answer_datetime IS NULL OR (is_scored_question AND question_type = 'datetime'));

-- Question-level score is a weight, never a penalty (the ANSWER-level
-- score deliberately allows negatives — penalty scoring lives there).
ALTER TABLE survey.survey_questions DROP CONSTRAINT IF EXISTS chk_questions_answer_score_nonneg;
ALTER TABLE survey.survey_questions ADD CONSTRAINT chk_questions_answer_score_nonneg
    CHECK (answer_score >= 0);

ALTER TABLE survey.survey_questions DROP CONSTRAINT IF EXISTS chk_questions_scale_band;
ALTER TABLE survey.survey_questions ADD CONSTRAINT chk_questions_scale_band
    CHECK (0 <= scale_min AND scale_min < scale_max AND scale_max <= 10);

ALTER TABLE survey.survey_questions DROP CONSTRAINT IF EXISTS chk_questions_time_limit_positive;
ALTER TABLE survey.survey_questions ADD CONSTRAINT chk_questions_time_limit_positive
    CHECK (NOT is_time_limited OR time_limit > 0);

-- Validation range families: lengths non-negative with min <= max; each
-- float/date/datetime pair ordered when both ends are present.
ALTER TABLE survey.survey_questions DROP CONSTRAINT IF EXISTS chk_questions_validation_length;
ALTER TABLE survey.survey_questions ADD CONSTRAINT chk_questions_validation_length
    CHECK (validation_length_min >= 0 AND validation_length_max >= 0
           AND validation_length_min <= validation_length_max);

ALTER TABLE survey.survey_questions DROP CONSTRAINT IF EXISTS chk_questions_validation_float_range;
ALTER TABLE survey.survey_questions ADD CONSTRAINT chk_questions_validation_float_range
    CHECK (validation_min_float_value IS NULL OR validation_max_float_value IS NULL
           OR validation_min_float_value <= validation_max_float_value);

ALTER TABLE survey.survey_questions DROP CONSTRAINT IF EXISTS chk_questions_validation_date_range;
ALTER TABLE survey.survey_questions ADD CONSTRAINT chk_questions_validation_date_range
    CHECK (validation_min_date IS NULL OR validation_max_date IS NULL
           OR validation_min_date <= validation_max_date);

ALTER TABLE survey.survey_questions DROP CONSTRAINT IF EXISTS chk_questions_validation_datetime_range;
ALTER TABLE survey.survey_questions ADD CONSTRAINT chk_questions_validation_datetime_range
    CHECK (validation_min_datetime IS NULL OR validation_max_datetime IS NULL
           OR validation_min_datetime <= validation_max_datetime);

-- ── survey.survey_question_answers ────────────────────────────────────────
-- Role XOR: a label is either a choice/matrix-column answer of one
-- question, or a matrix row of another — exactly one.
ALTER TABLE survey.survey_question_answers DROP CONSTRAINT IF EXISTS chk_answers_role_xor;
ALTER TABLE survey.survey_question_answers ADD CONSTRAINT chk_answers_role_xor
    CHECK ((question_id IS NULL) <> (matrix_question_id IS NULL));

-- A label carries display content: a value string or an image filename.
ALTER TABLE survey.survey_question_answers DROP CONSTRAINT IF EXISTS chk_answers_value_present;
ALTER TABLE survey.survey_question_answers ADD CONSTRAINT chk_answers_value_present
    CHECK (value IS NOT NULL OR value_image_filename IS NOT NULL);

-- ── Tier A token_nonce uniqueness among live rows ─────────────────────────
CREATE UNIQUE INDEX IF NOT EXISTS idx_user_inputs_token_nonce_live
    ON survey.survey_user_inputs (token_nonce)
    WHERE (metadata->>'deleted_at') IS NULL;

-- ── question DELETE block during a live session ───────────────────────────
-- The session cursor (surveys.session_question_id) points at a question;
-- deleting questions mid-session corrupts the running session. Soft-delete
-- is the normal path; this guard refuses the hard DELETE.
CREATE OR REPLACE FUNCTION survey.survey_refuse_question_delete_in_session() RETURNS trigger AS $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM survey.survey_surveys s
        WHERE s.id = OLD.survey_id
          AND s.session_state = 'in_progress'
    ) THEN
        RAISE EXCEPTION 'cannot delete questions of a survey while its session is in progress (question %)', OLD.id
            USING ERRCODE = '23510',
                  HINT = 'end the session first, or soft-delete the question';
    END IF;
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS survey_question_delete_in_session ON survey.survey_questions;
CREATE TRIGGER survey_question_delete_in_session
    BEFORE DELETE ON survey.survey_questions
    FOR EACH ROW EXECUTE FUNCTION survey.survey_refuse_question_delete_in_session();

-- ── attempt-pool + leaderboard indexes ────────────────────────────────────
-- The pool join reads: same survey, state = done, not a test entry, live
-- row, shared invite_token (or both NULL) — the leading arm:
CREATE INDEX IF NOT EXISTS idx_user_inputs_pool_invite
    ON survey.survey_user_inputs (survey_id, invite_token)
    WHERE state = 'done' AND test_entry IS NOT TRUE AND (metadata->>'deleted_at') IS NULL;

-- The leaderboard ranks done non-test attempts by raw total:
CREATE INDEX IF NOT EXISTS idx_user_inputs_leaderboard_total
    ON survey.survey_user_inputs (survey_id, scoring_total DESC)
    WHERE state = 'done' AND test_entry IS NOT TRUE AND (metadata->>'deleted_at') IS NULL;
