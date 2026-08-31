-- Reverse the hardening stamp: indexes, the delete-block trigger, and the
-- named CHECK constraints, newest-first in declaration order.

DROP INDEX IF EXISTS survey.idx_user_inputs_leaderboard_total;
DROP INDEX IF EXISTS survey.idx_user_inputs_pool_invite;

DROP TRIGGER IF EXISTS survey_question_delete_in_session ON survey.survey_questions;
DROP FUNCTION IF EXISTS survey.survey_refuse_question_delete_in_session();

DROP INDEX IF EXISTS survey.idx_user_inputs_token_nonce_live;

ALTER TABLE survey.survey_question_answers DROP CONSTRAINT IF EXISTS chk_answers_value_present;
ALTER TABLE survey.survey_question_answers DROP CONSTRAINT IF EXISTS chk_answers_role_xor;

ALTER TABLE survey.survey_questions DROP CONSTRAINT IF EXISTS chk_questions_validation_datetime_range;
ALTER TABLE survey.survey_questions DROP CONSTRAINT IF EXISTS chk_questions_validation_date_range;
ALTER TABLE survey.survey_questions DROP CONSTRAINT IF EXISTS chk_questions_validation_float_range;
ALTER TABLE survey.survey_questions DROP CONSTRAINT IF EXISTS chk_questions_validation_length;
ALTER TABLE survey.survey_questions DROP CONSTRAINT IF EXISTS chk_questions_time_limit_positive;
ALTER TABLE survey.survey_questions DROP CONSTRAINT IF EXISTS chk_questions_scale_band;
ALTER TABLE survey.survey_questions DROP CONSTRAINT IF EXISTS chk_questions_answer_score_nonneg;
ALTER TABLE survey.survey_questions DROP CONSTRAINT IF EXISTS chk_questions_answer_datetime_typed;
ALTER TABLE survey.survey_questions DROP CONSTRAINT IF EXISTS chk_questions_answer_date_typed;
ALTER TABLE survey.survey_questions DROP CONSTRAINT IF EXISTS chk_questions_page_xor;

ALTER TABLE survey.survey_surveys DROP CONSTRAINT IF EXISTS chk_surveys_speed_rating_limit;
ALTER TABLE survey.survey_surveys DROP CONSTRAINT IF EXISTS chk_surveys_time_limit_positive;
ALTER TABLE survey.survey_surveys DROP CONSTRAINT IF EXISTS chk_surveys_attempts_limit_positive;
ALTER TABLE survey.survey_surveys DROP CONSTRAINT IF EXISTS chk_surveys_certification_scoring;
ALTER TABLE survey.survey_surveys DROP CONSTRAINT IF EXISTS chk_surveys_success_min_band;
