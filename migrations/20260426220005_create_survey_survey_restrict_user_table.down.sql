-- Down: drop survey.survey_survey_restrict_users table
DROP TABLE IF EXISTS survey.survey_survey_restrict_users CASCADE;
DROP FUNCTION IF EXISTS survey.survey_survey_restrict_users_audit_timestamp() CASCADE;
