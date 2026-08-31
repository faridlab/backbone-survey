-- Down: drop enum types for survey module
DROP TYPE IF EXISTS survey_answer_type CASCADE;
DROP TYPE IF EXISTS survey_input_state CASCADE;
DROP TYPE IF EXISTS survey_invite_existing_mode CASCADE;
DROP TYPE IF EXISTS survey_progression_mode CASCADE;
DROP TYPE IF EXISTS survey_report_layout CASCADE;
DROP TYPE IF EXISTS survey_questions_selection CASCADE;
DROP TYPE IF EXISTS survey_questions_layout CASCADE;
DROP TYPE IF EXISTS survey_session_state CASCADE;
DROP TYPE IF EXISTS survey_scoring_type CASCADE;
DROP TYPE IF EXISTS survey_access_mode CASCADE;
DROP TYPE IF EXISTS survey_survey_type CASCADE;
DROP TYPE IF EXISTS survey_matrix_subtype CASCADE;
DROP TYPE IF EXISTS survey_question_type CASCADE;
