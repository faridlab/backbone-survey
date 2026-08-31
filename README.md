# backbone-survey

The survey/questionnaire module: composed questionnaires with conditional
branching, scored certifications with a frozen per-attempt denominator, and
human-typed live sessions with speed-rated answering. Ported from Odoo
`survey` with the deviations recorded in `SPEC.md` (the source of record for
every decision).

## Shape

- **Schema** `survey`, no company fence (single-estate data), enums live
  unqualified in `public` (house convention).
- **9 tables**: `survey_surveys`, `survey_questions` (dual-natured: pages and
  questions share one table and one sequence space), `survey_question_answers`,
  `survey_question_triggering_answers` (the only stored conditional edge),
  `survey_survey_restrict_users` + `survey_survey_langs` (junction entities),
  `survey_user_inputs` (the attempt), `survey_user_input_lines` (the answer
  line), `survey_user_input_predefined_questions` (the frozen denominator).
- **13 enums**, all created idempotently in `20260426220000_create_enums`.
- **Zero scheduled jobs** and **zero module-owned outbox events** — expiry is
  read-path, session advance is verb push, and everything outbound crosses a
  host-composed seam (below).

## The two host seams

The module has no Cargo edge on engagement (or any website host):

1. **CertificationGrantPort** (`src/application/service/certification_port.rs`)
   — delivers the `CertificationPassed` fact toward engagement's exactly-once
   badge grant. Deny-by-default: until the host calls
   `SurveyModule::set_certification_grant(...)`, every publish refuses loudly
   with `CertificationGrantError::NotComposed` and is recorded as the audited
   critical event `survey_certification_grant_refused`. The fact mirrors the
   contract pinned in engagement's `schema/hooks/index.hook.yaml`.
2. **SurveyEventSink** (`src/application/service/event_sink.rs`) — the
   notification facts (participant invited with the Tier A link, answer
   completed, session started/advanced/ended). Default implementation logs;
   the host's implementation stages them onto the record-shaped realtime
   channel `survey.survey_{survey_id}` or its mail path.

## Security model (ADR-0018)

- **Tier A** — the per-attempt capability ON `survey_user_inputs`:
  `token_nonce` selector + mandatory `token_expires_at`; the public link
  carries `{input_id}.{nonce}.{exp}.{mac}` with an HMAC-SHA256 MAC over
  (id, nonce, grant, exp). Multi-use within the attempt; rotation mints a
  fresh nonce atomically. `access_token` on the survey is a public URL key,
  NOT a credential.
- **Tier B** — human-typed `session_code` for live sessions: uniqueness is a
  partial UNIQUE, growth is 4→9 digits on collision with loud exhaustion,
  and verify failures feed an escalating lockout.

## Invariants enforced at the DB

- **Monotonic attempt state** (`20260831000001_survey_input_monotonic_guard`):
  a BEFORE UPDATE trigger refuses any backward `state` edge (new=1,
  in_progress=2, done=3) — raw SQL included.
- **Score drift refusal** (same stamp): `answer_score` / `answer_is_correct`
  / `speed_seconds` are written once at submit; any later change raises
  unless the regrade verb set `survey.allow_regrade = 'on'` for its
  transaction.
- **Hardening** (`20260831000002_survey_hardening`): the dual-nature and
  answer-role XORs, the scoring/certification/scale/validation CHECK
  families, the live-rows partial UNIQUE on `token_nonce`, the question
  DELETE block while a session is in progress, and the attempt-pool +
  leaderboard indexes.

## Reading notes

- `invite_token` is deliberately NOT unique — it names an attempt POOL;
  attempt counting is the self-join documented in `SPEC.md` §2.6.
- `certification_badge_key` replaces Odoo's badge FK with engagement's
  stable key; a typo'd key surfaces at publication time as the audited
  refusal, not an FK error.
- The stored aggregates keep the upstream asymmetry: `scoring_total` can go
  negative (leaderboard ranks on it); `scoring_percentage` is 0-floored.
- `is_scored_question` ports as an explicit flag so a `0.0` correct answer
  is scoreable (upstream scalar truthiness dropped it).
- KPI computes exclude `test_entry` rows (a deliberate deviation from
  upstream).

## Layout

```
schema/            the source of truth: models + hooks (rules R-SV*, the
                   attempt state machine, lifecycle declarations)
src/               generated (entities, repos, services, routes) + the two
                   hand service files under user_owned
migrations/        codegen stamps 20260426220000-012 + hand stamps
                   20260831000001/2 (monotonic guard, hardening)
config/            application.yml (+ -dev/-prod): token TTLs, code bounds,
                   lockout constants
```

Regenerate with `metaphor schema generate --force` from this directory; the
`user_owned` globs in `metaphor.codegen.yaml` protect every hand file.
