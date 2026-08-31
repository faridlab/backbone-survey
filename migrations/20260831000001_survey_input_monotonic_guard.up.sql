-- Monotonic guards for the survey attempt cluster, enforced at the DB level.
--
-- Two triggers, one shared escape hatch:
--
-- 1. survey_input_monotonic_guard (BEFORE UPDATE on survey.survey_user_inputs):
--    the attempt state may only advance (new -> in_progress -> done). Any
--    backward edge raises, whatever the write path — the transition verbs,
--    the session-end bulk write, batch tools, or raw SQL. A partial UNIQUE
--    cannot express a directional per-row transition and a CHECK cannot see
--    the prior row, so a trigger is the only DB-level shape for "may only
--    advance".
--
-- 2. survey_score_drift_refused (BEFORE UPDATE on survey.survey_user_input_lines):
--    the score payload (answer_score, answer_is_correct, speed_seconds) is
--    written once at submit and is immutable afterwards. Any later change
--    raises, outside the sanctioned regrade marker.
--
-- The regrade marker: the guarded regrade_question verb recomputes a line's
-- score from the STORED speed_seconds and value columns (never a wall clock)
-- and sets 'survey.allow_regrade' = 'on' for its transaction via set_config
-- (SET LOCAL survey.allow_regrade = 'on'). Both triggers ignore the marker —
-- only the drift trigger consults it.

-- State rank: new=1, in_progress=2, done=3.
CREATE OR REPLACE FUNCTION survey.survey_input_state_rank(state survey_input_state) RETURNS integer AS $$
    SELECT CASE state
        WHEN 'new' THEN 1
        WHEN 'in_progress' THEN 2
        WHEN 'done' THEN 3
    END;
$$ LANGUAGE sql IMMUTABLE;

-- The monotonic guard itself.
CREATE OR REPLACE FUNCTION survey.survey_input_monotonic_guard() RETURNS trigger AS $$
BEGIN
    IF NEW.state IS DISTINCT FROM OLD.state
       AND survey.survey_input_state_rank(NEW.state) < survey.survey_input_state_rank(OLD.state) THEN
        RAISE EXCEPTION 'survey input state is not monotonic (input %: % -> %)', OLD.id, OLD.state, NEW.state
            USING ERRCODE = '23514',
                  HINT = 'attempt state may only advance new -> in_progress -> done';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS survey_input_monotonic_guard ON survey.survey_user_inputs;
CREATE TRIGGER survey_input_monotonic_guard
    BEFORE UPDATE ON survey.survey_user_inputs
    FOR EACH ROW EXECUTE FUNCTION survey.survey_input_monotonic_guard();

-- The score-drift refusal: immutable score payload outside the regrade marker.
-- OLD.answered_at IS NOT NULL identifies a persisted line (the column is
-- NOT NULL DEFAULT now(), so every stored row qualifies).
CREATE OR REPLACE FUNCTION survey.survey_refuse_score_drift() RETURNS trigger AS $$
BEGIN
    IF OLD.answered_at IS NOT NULL
       AND (NEW.answer_score, NEW.answer_is_correct, NEW.speed_seconds)
           IS DISTINCT FROM
           (OLD.answer_score, OLD.answer_is_correct, OLD.speed_seconds)
       AND coalesce(current_setting('survey.allow_regrade', true), 'off') <> 'on' THEN
        RAISE EXCEPTION 'survey score drift refused (line %)', OLD.id
            USING ERRCODE = '23514',
                  HINT = 'answer_score/answer_is_correct/speed_seconds are written once at submit; use the regrade_question verb to recompute';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS survey_score_drift_refused ON survey.survey_user_input_lines;
CREATE TRIGGER survey_score_drift_refused
    BEFORE UPDATE ON survey.survey_user_input_lines
    FOR EACH ROW EXECUTE FUNCTION survey.survey_refuse_score_drift();
