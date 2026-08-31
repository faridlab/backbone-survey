-- A certification survey never carries a session state.
--
-- The service layer refuses at arm time (the typed
-- SessionCertificationConflict); this CHECK holds the same line against
-- every OTHER writer — raw SQL, a hand-edited officer update, a future
-- code path that sets session_state without passing through arm_session.

ALTER TABLE survey.survey_surveys
    ADD CONSTRAINT survey_session_certification_disjoint
    CHECK (NOT (certification AND session_state IS NOT NULL));
