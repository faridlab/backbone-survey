-- Down: drop survey.survey_user_inputs table
DROP TABLE IF EXISTS survey.survey_user_inputs CASCADE;
DROP FUNCTION IF EXISTS survey.survey_user_inputs_audit_timestamp() CASCADE;
