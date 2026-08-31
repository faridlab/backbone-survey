-- Down: drop survey.survey_user_input_predefined_questions table
DROP TABLE IF EXISTS survey.survey_user_input_predefined_questions CASCADE;
DROP FUNCTION IF EXISTS survey.survey_user_input_predefined_questions_audit_timestamp() CASCADE;
