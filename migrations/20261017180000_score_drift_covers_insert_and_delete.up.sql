-- A finished attempt's graded lines are fixed: besides an update to a line's score (refused since
-- the monotonic guard), adding a line to a done attempt or deleting one of its lines is refused,
-- outside the regrade marker, so a delete-and-reinsert cannot rewrite a score after submit. While
-- an attempt is open a choice answer still replaces its lines. Deleting the attempt itself takes
-- its lines with it: once the attempt row is gone the guard lets them go.
--
-- Both refusals now carry the rule's constraint name, so they answer survey_score_drift_refused.
CREATE OR REPLACE FUNCTION survey.survey_refuse_score_drift() RETURNS trigger AS $$
BEGIN
    IF OLD.answered_at IS NOT NULL
       AND (NEW.answer_score, NEW.answer_is_correct, NEW.speed_seconds)
           IS DISTINCT FROM
           (OLD.answer_score, OLD.answer_is_correct, OLD.speed_seconds)
       AND coalesce(current_setting('survey.allow_regrade', true), 'off') <> 'on' THEN
        RAISE EXCEPTION 'survey score drift refused (line %)', OLD.id
            USING ERRCODE = 'check_violation',
                  CONSTRAINT = 'ck_survey_user_input_lines_score_drift_refused__survey_score_drift_refused',
                  HINT = 'answer_score/answer_is_correct/speed_seconds are written once at submit; use the regrade_question verb to recompute';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION survey.survey_refuse_done_line_churn() RETURNS trigger AS $$
DECLARE
    line survey.survey_user_input_lines%ROWTYPE;
BEGIN
    IF TG_OP = 'DELETE' THEN line := OLD; ELSE line := NEW; END IF;
    IF coalesce(current_setting('survey.allow_regrade', true), 'off') <> 'on'
       AND EXISTS (SELECT 1 FROM survey.survey_user_inputs i
                    WHERE i.id = line.user_input_id AND i.state = 'done') THEN
        RAISE EXCEPTION 'survey score drift refused: attempt % is done (line %)', line.user_input_id, line.id
            USING ERRCODE = 'check_violation',
                  CONSTRAINT = 'ck_survey_user_input_lines_score_drift_refused__survey_score_drift_refused',
                  HINT = 'a finished attempt''s lines are fixed; use the regrade_question verb to recompute';
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS survey_done_line_churn_refused ON survey.survey_user_input_lines;
CREATE TRIGGER survey_done_line_churn_refused
    BEFORE INSERT OR DELETE ON survey.survey_user_input_lines
    FOR EACH ROW EXECUTE FUNCTION survey.survey_refuse_done_line_churn();
