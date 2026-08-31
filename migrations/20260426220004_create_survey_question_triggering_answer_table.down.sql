-- Down: drop survey.survey_question_triggering_answers table
DROP TABLE IF EXISTS survey.survey_question_triggering_answers CASCADE;
DROP FUNCTION IF EXISTS survey.survey_question_triggering_answers_audit_timestamp() CASCADE;
