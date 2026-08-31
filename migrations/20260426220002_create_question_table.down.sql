-- Down: drop survey.survey_questions table
DROP TABLE IF EXISTS survey.survey_questions CASCADE;
DROP FUNCTION IF EXISTS survey.survey_questions_audit_timestamp() CASCADE;
