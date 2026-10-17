DROP TRIGGER IF EXISTS survey_done_line_churn_refused ON survey.survey_user_input_lines;
DROP FUNCTION IF EXISTS survey.survey_refuse_done_line_churn();
