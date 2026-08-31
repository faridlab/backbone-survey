-- Down: drop survey.survey_survey_langs table
DROP TABLE IF EXISTS survey.survey_survey_langs CASCADE;
DROP FUNCTION IF EXISTS survey.survey_survey_langs_audit_timestamp() CASCADE;
