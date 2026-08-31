-- Down: drop survey.survey_surveys table
DROP TABLE IF EXISTS survey.survey_surveys CASCADE;
DROP FUNCTION IF EXISTS survey.survey_surveys_audit_timestamp() CASCADE;
