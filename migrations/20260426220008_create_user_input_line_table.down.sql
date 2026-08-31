-- Down: drop survey.survey_user_input_lines table
DROP TABLE IF EXISTS survey.survey_user_input_lines CASCADE;
DROP FUNCTION IF EXISTS survey.survey_user_input_lines_audit_timestamp() CASCADE;
