-- Reverse the monotonic guards (state rank helper + both triggers).

DROP TRIGGER IF EXISTS survey_score_drift_refused ON survey.survey_user_input_lines;
DROP FUNCTION IF EXISTS survey.survey_refuse_score_drift();

DROP TRIGGER IF EXISTS survey_input_monotonic_guard ON survey.survey_user_inputs;
DROP FUNCTION IF EXISTS survey.survey_input_monotonic_guard();

DROP FUNCTION IF EXISTS survey.survey_input_state_rank(survey_input_state);
