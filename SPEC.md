# backbone-survey — Module Specification

> **Status:** SPEC (implementation source of truth). Authored against the tree state of
> 2026-08-31. Every later seat (schema, service, routes, tests, docs) implements THIS
> document; conflicts between this spec and ad-hoc reads of the Odoo source are resolved
> here first, then by the pillar plan (`docs/plan/07-pillar-marketing.md` M-4), then by the
> council record (`docs/council/2026-08-29-module-w6-p0-platform-pass.md` conditions 11–13).

## 0. Spec sources and anchors

Primary (Odoo 19 community, `addons/survey`, commit `b9eb72eb`, cycle 9):

| Anchor | File |
|---|---|
| SVM-1…SVM-11, SV-B1…B6, TR-SV-1…10, SV-S1…S15, SV-C1…C5, SV-D1 | `docs/odoo/marketing/survey/survey-business-logic.md`, `docs/odoo/marketing/survey/schema/hooks/survey.hook.yaml` |
| Field-by-field schemas | `docs/odoo/marketing/survey/schema/models/{survey,question,user_input,extensions,index}.model.yaml` |
| ACL / groups / routes / templates posture | `docs/odoo/marketing/survey/survey-features.md` |
| Faithfulness checklist | `docs/odoo/marketing/survey/README.md` (notes 1–12) |

House standards:

| Standard | Anchor |
|---|---|
| Lifecycle field vocabulary (pattern 2 = `hand_set`) | `docs/handbook/adr/0016-lifecycle-field-declaration.md` |
| Bus = outbox/inbox role, channel keys, BUS-B2/B5 | `docs/handbook/adr/0017-bus-maps-onto-outbox-inbox.md` |
| Two-tier capability tokens | `docs/handbook/adr/0018-two-tier-capability-token-standard.md` |
| HTTP surface: safe methods, `action_link` class, throttled oracles | `docs/handbook/adr/0019-http-surface-standard.md` |

Implemented precedents in-tree (verified 2026-08-31):

| Precedent | Anchor |
|---|---|
| Tier A capability on-row (nonce selector + mandatory expiry + HMAC MAC over `(id, nonce, grant, exp)`, single-use conditional UPDATE, shared no-oracle refusal) | `modules/backbone-engagement/schema/models/rating.model.yaml` (token_nonce / token_expires_at), `modules/backbone-engagement/src/presentation/http/rating_routes.rs` (`public_composer`, 120/min, `not_submittable`) |
| Tier B lockout set (3 attempts, 30 s base doubling, 15 min cap, 1 s spacing, shape check) | `modules/backbone-attendance/src/application/service/attendance_write_service.rs:32-70` |
| Exactly-once event grants, `grant_key` partial UNIQUE, inbound `CertificationPassed` consumer | `modules/backbone-engagement/src/application/service/gamification_write_service.rs:784-817` (`on_certification_passed`), `modules/backbone-engagement/schema/hooks/index.hook.yaml:58-83` |
| Fail-closed host-composed port (module owns trait + refusing default, host wires adapter) | `modules/backbone-mail/src/application/service/phone_ports.rs` (`PhoneBookPort` / `RefusingPhoneBook` / `PhoneSourceError::NotComposed`) |
| Realtime record-channel grammar + resolver walk + guest identity | `modules/backbone-mail/src/domain/event/constants.rs:38` (`record_channel` = `{model}_{id}`), `modules/backbone-mail/src/realtime/tailer.rs:60-64` (`parse_record_key`), `modules/backbone-mail/src/realtime/sse.rs:88-107` (allowlist → `ThreadAccessResolver`), `modules/backbone-mail/src/application/service/chatter_acl.rs:75` (trait), `modules/backbone-mail/src/presentation/http/guest_routes.rs:98` (`POST /mail/guest`) |
| NEW-module manifest layout, fail-hard scratch-DB harness, hand-authored route/service files under `user_owned:` | `modules/backbone-mailing/{Cargo.toml,metaphor.codegen.yaml,tests/}` |

## 1. Module identity

| Key | Value |
|---|---|
| Module (schema) name | `survey` — tables live in the `survey` Postgres schema; routes mount under `/api/v1/survey` (guarded) and a bare root-level public family (§9) |
| Crate | `backbone-survey` (lib only, no `main.rs`, DDD 4-layer per the module CLAUDE.md) |
| Future repository | `github.com/faridlab/backbone-survey` — created by the orchestrator at train time; this directory is the working tree until then |
| Framework pins | `backbone-core`, `backbone-orm`, `backbone-auth`, `backbone-messaging`, `backbone-rate-limit` — **all `tag = "v2.7.11"`** (framework `v2.7.11` = `5767679`, "Qualify ?include= relation targets with the caller's schema at hydration", verified in-tree) |
| Sibling Cargo deps | **NONE.** Zero path deps, zero git deps on sibling modules (contrast mailing→mail). The certification seam is a fail-closed port trait (§7); the realtime seam is a host-composed sink (§8); invitations are host-relayed facts (§9) |
| Fence | `none` (single-tenant family posture C2) — **no `company_id` columns synthesized anywhere** |
| Enums | Created UNQUALIFIED in the `public` schema (module convention, ratified), collision-checked in §4 |
| Scheduled jobs | **ZERO** (§11) — no own outbox schema, no `outbox_schemas` host-config change |
| First release | `0.1.0` (tag cut is the orchestrator's, owner-gated) |
| Dep-edge assertion (wave DoD) | The crate's manifest + resolve graph gains **no edge onto `backbone-portal` or any website-routing host**; `pin-probe.sh` extension per council condition 18 |

## 2. Entities

Five persisted entities. Odoo model → entity mapping follows the spec source's map
(`docs/odoo/marketing/survey/schema/models/index.model.yaml`):

| Odoo | Entity | Collection | HTTP surface |
|---|---|---|---|
| `survey.survey` | `Survey` | `survey_surveys` | guarded CRUD + verbs |
| `survey.question` (dual-natured: questions AND pages) | `Question` | `survey_questions` | guarded CRUD + verbs |
| `survey.question.answer` (labels: choice answers / matrix columns / matrix rows) | `QuestionAnswer` | `survey_question_answers` | guarded CRUD |
| `survey.user_input` | `UserInput` | `survey_user_inputs` | **read-only CRUD**; writes only through intake/verb paths |
| `survey.user_input.line` | `UserInputLine` | `survey_user_input_lines` | **read-only CRUD** (system-written — mirrors the upstream 1/0/0/0 officer ACL) |

**Not ported as tables (recorded):**
- `survey.invite` (TransientModel) → the invitation flow is a typed `invite` verb on the
  write service (§9) + host-relayed facts. Its `existing_mode` selection survives as the
  verb's enum parameter.
- A separate `Session` entity does **not** exist upstream and is not invented here: the
  live-session runtime is **survey-carried** (`session_state`, `session_code`, cursor,
  clocks — SVM-1, SVB §2). The plan's "sessions" scope line is satisfied by that runtime
  plus the realtime composition (§8). This is a faithful-port reading, not a trim.
- Host extensions (`gamification.badge.survey_id`, `challenge` selection_add,
  `res.partner`/`res.lang`/`ir.http`) do not port as edges: badges are referenced by
  **stable key string** (§7), language forcing is the substituted webapp concern
  (council-decided), and no cross-schema column is written by this module.

### 2.1 Survey (`survey.survey` — source `schema/models/survey.model.yaml`)

| Column | Type | Constraints / notes |
|---|---|---|
| id | uuid PK | `@default(uuid)` |
| survey_type | enum `survey_survey_type` | NOT NULL default `custom`. Template STYLE, **not** a lifecycle (SVM-1) |
| title | string | NOT NULL |
| description / description_done | text? | intro / completion message payloads |
| background_image | string? | asset key (binary storage is the bucket module's; the column carries the reference) |
| active | boolean | NOT NULL default true. **THIS IS THE 'closed' STATE** — enforced functionally at the intake gate; there is no `state` column and no status enum (SVM-1; register row SVM-1) |
| user_id | uuid? | logical ref `sapiens.User` (no FK) — responsible |
| restrict_user_ids | m2m → sapiens.User | join table; the officer visibility allow-list |
| lang_ids | m2m → i18n.Language | logical refs; the survey's language allow-list (kept as data for the webapp; see dispositions) |
| questions_layout | enum `survey_questions_layout` | NOT NULL default `page_per_question`; session start forces `page_per_question` |
| questions_selection | enum `survey_questions_selection` | NOT NULL default `all` (`random` ignores conditionals — SVM-7) |
| access_mode | enum `survey_access_mode` | NOT NULL default `public`. Odoo's dead `authentication`/`internal` branches do NOT port |
| access_token | string | NOT NULL UNIQUE default uuid4 — **public URL key, not a credential** (SVF) |
| users_login_required | boolean | gates the certification guard below |
| users_can_go_back | boolean | roaming; load-bearing intake semantics (mandatory-empty only errors when NOT roaming) |
| scoring_type | enum `survey_scoring_type` | NOT NULL default `no_scoring`. **Clamp guard** (SVM-2): certification forces `scoring_without_answers`; `no_scoring` forces `certification = false` |
| scoring_success_min | float | NOT NULL default 80.0, CHECK `0 <= x <= 100` (SV-S4) |
| is_attempts_limited | boolean | **Clamp guard**: forced true when the survey has conditional questions or login-required token access |
| attempts_limit | integer | default 1, CHECK positive when limited (SV-S6) |
| is_time_limited | boolean | survey-wide time limit toggle |
| time_limit | float | minutes, default 10, CHECK positive when limited (SV-S5). Enforced server-side with the +10 s grace (§5) |
| certification | boolean | **Clamp guard** + CHECK interlock `certification ⇒ scoring_type ≠ no_scoring` (SV-S3, both directions covered by clamp) |
| certification_mail_template_id | uuid? | logical ref `messaging.MailTemplate` — config only; sending is host-relayed |
| certification_report_layout | enum `survey_report_layout` | default `modern_purple`; the webapp certification page's style selector (§10 disposition) |
| certification_give_badge | boolean | **Clamp guard**: true iff `users_login_required AND certification` |
| certification_badge_key | string? | **DEVIATION (recorded): replaces Odoo's `certification_badge_id` FK.** The badge's stable key in engagement, resolved by the host adapter (§7). UNIQUE where NOT NULL — the `_badge_uniq` global 1:1 (one badge is the certification badge of at most one survey, ever; SV-S7) |
| session_state | enum `survey_session_state` | NULL. **The only selection machine on this model — hand-set** (`lifecycle: shape: hand_set`, ADR-0016 pattern 2; NULL = none/ended) |
| session_code | string? | UNIQUE where NOT NULL (SV-S2). Tier B human-typed join key (§5.2). Clamp-guard-generated on arm, loudly exhaustive (SV-B12) |
| session_question_id | uuid? | logical ref `Question` — THE session cursor |
| session_start_time | datetime? | leaderboard window lower bound |
| session_question_start_time | datetime? | speed-rating clock; written `now + 1 s` while the pushed payload carries pre-write millis (SVM-11; deliberate skew, kept) |
| session_speed_rating | boolean | CHECK: on ⇒ positive `session_speed_rating_time_limit` (SV-S8) |
| session_speed_rating_time_limit | integer? | seconds for full speed points; propagated to questions by the update verb (deliberately NOT derived — circular) |
| audit metadata | house standard | created/updated actor + timestamps |

Indexes: `unique(access_token)`; `unique(session_code) WHERE session_code IS NOT NULL`;
`unique(certification_badge_key) WHERE certification_badge_key IS NOT NULL`;
btree `(active, survey_type)` for the officer list.

Non-stored reads (read service, NOT columns — SVM-10/register row): `answer_count`,
`answer_done_count`, `answer_score_avg`, `answer_duration_avg`, `success_count`,
`success_ratio`, `scoring_max_obtainable`, `session_show_leaderboard`,
`has_conditional_questions`. **KPI computes exclude
`test_entry` rows** (SV-B5 fixed deliberately; recorded deviation from upstream).

### 2.2 Question — dual-natured with pages (SVM-5)

Source `schema/models/question.model.yaml`. Ported **verbatim as one table** (register row
SVM-5 "port verbatim" chosen over split):

| Column | Type | Constraints / notes |
|---|---|---|
| id | uuid PK | |
| survey_id | uuid NOT NULL | FK → `survey.surveys` ON DELETE CASCADE, indexed |
| sequence | integer | NOT NULL default 10; pages and questions share ONE sequence space (conditional triggers must reference earlier sequence) |
| is_page | boolean | NOT NULL default false — the dual-nature switch |
| question_type | enum `survey_question_type`? | CHECK `(is_page = (question_type IS NULL))` — the SV-C3 XOR made DB-level (upstream was ORM-only; strengthening recorded). Pages have NULL type; questions default `simple_choice`. **Clamp guard** (SVM-2) |
| title | string | NOT NULL |
| description | text? | |
| question_placeholder | string? | clamp: cleared for choice/matrix |
| background_image | string? | clamp: pages only |
| random_questions_count | integer | default 1 (`questions_selection = random` only) |
| is_scored_question | boolean | **Clamp guard** — scalar truthiness upstream; the port scores an explicit `Option<f64>` equality so **`0.0` correct answers ARE scoreable** (recorded deviation, README note 9) |
| answer_numerical_box | float? | correct numerical answer (0.0 valid — see above) |
| answer_date | date? / answer_datetime | datetime? — CHECK: set when scored + type date/datetime (SV-S11) |
| answer_score | float | default 0, CHECK `>= 0` (SV-S10 — QUESTION-level only; the answer-level field deliberately allows negatives) |
| save_as_email / save_as_nickname | boolean | intake side-writes onto the input row |
| matrix_subtype | enum `survey_matrix_subtype` | default `simple` |
| scale_min / scale_max | integer | defaults 0/10, CHECK `0 <= min < max <= 10` (SV-S12) |
| scale_min_label / scale_mid_label / scale_max_label | string? | |
| is_time_limited | boolean + time_limit integer? | CHECK positive when limited (SV-S13); `is_time_customized` boolean recorded when diverging from survey defaults |
| comments_allowed / comments_message / comment_count_as_answer | boolean / string? / boolean | |
| validation_required | boolean | **Clamp guard**: forced false outside char/numerical/date/datetime |
| validation_email | boolean | char_box email check |
| validation_length_min / validation_length_max | integer | defaults 0, CHECK `>= 0` and `min <= max` (SV-S9 family) |
| validation_min_float_value / validation_max_float_value | float | CHECK `min <= max` |
| validation_min_date / validation_max_date, validation_min_datetime / validation_max_datetime | date? / datetime? | CHECK `min <= max` each pair |
| validation_error_msg / constr_error_msg | string? | |
| constr_mandatory | boolean | empty-mandatory errors only when NOT `users_can_go_back` |
| triggering_answer_ids | m2m → QuestionAnswer | **THE only stored conditional edge** (SVM-7); join table `survey_question_triggering_answers`. Domain restriction (answers of earlier simple/multiple-choice questions) enforced by the write service |
| page_id | uuid? | logical self-ref — **write-maintained derivation** (SVM-8): recomputed by the question upsert/reorder verbs as "last page before this question in sequence order". No ORM recompute exists to drift; a consistency probe asserts it (§12) |

### 2.3 QuestionAnswer (labels)

| Column | Type | Constraints / notes |
|---|---|---|
| id | uuid PK | |
| question_id | uuid? | choice/matrix-column role |
| matrix_question_id | uuid? | matrix-row role. CHECK `((question_id IS NULL) <> (matrix_question_id IS NULL))` — the SV-C4 XOR made DB-level |
| sequence | integer | NOT NULL default 10 |
| value | string? | CHECK `value IS NOT NULL OR value_image_filename IS NOT NULL` (SV-S14) |
| value_image | string? / value_image_filename | string? — asset reference pair |
| is_correct | boolean | |
| answer_score | float | **negatives deliberately allowed** (penalty scoring) |

### 2.4 UserInput (the attempt)

| Column | Type | Constraints / notes |
|---|---|---|
| id | uuid PK | |
| survey_id | uuid NOT NULL | FK cascade, indexed |
| token_nonce | string | NOT NULL, UNIQUE where live — **Tier A selector** (§5.1). The public link carries `{id}.{nonce}.{exp}.{mac}` |
| token_expires_at | datetime | NOT NULL — mandatory expiry (ADR-0018 Tier A; the upstream never-expiring uuid4 is the named non-conformance) |
| invite_token | string? | **deliberately NOT UNIQUE** — names an attempt POOL (README note 5); attempt counting is the raw-SQL self-join (§2.6) |
| partner_id | uuid? | logical ref `party.Party` |
| email | string? | identity half B of the pool join; written by `save_as_email` side-write |
| nickname | string? | leaderboard identity (`save_as_nickname`) |
| user_id | uuid? | logical ref `sapiens.User` — the authenticated taker; **required for certification publication** (§7) |
| wire_identity_key | string? | indexed — opaque host-minted realtime handle (guest id or user key), stamped at join/begin through the host composition (§8). No FK; the module never interprets it |
| test_entry | boolean | NOT NULL default false — excluded from attempt counts AND (deviation, SV-B5) from KPI computes |
| state | enum `survey_input_state` | NOT NULL default `new`. `lifecycle: shape: hand_set`, machine `survey_input_state` (§3) + **DB-level monotonic trigger** (§6) |
| start_datetime / end_datetime | datetime? | stamped by the transition verbs |
| deadline | datetime? | compared **in UTC** on every gate (SV-B1 fixed; the naive `datetime.now()` defect does not port) |
| is_session_answer | boolean | gates speed-rating eligibility |
| last_displayed_page_id | uuid? | logical ref `Question` — the resume cursor |
| scoring_percentage | float | NOT NULL default 0 — stored, recomputed by the scoring service on line writes; 0-floored |
| scoring_total | float | NOT NULL default 0 — stored, **can go negative** (SVM-10 asymmetry kept: leaderboard ranks by raw total, statistics read the clamped percentage) |
| scoring_success | boolean | NOT NULL default false — `percentage >= survey.scoring_success_min`, evaluated live (mid-attempt too) |
| user_input_line_ids | o2m | the answers |
| predefined_question_ids | m2m → Question | **THE frozen denominator snapshot** (§5.3); join table `survey_user_input_predefined_questions` |

Dropped from upstream (recorded): `lang_id` (rendering concern, webapp owns language),
`survey_first_submitted` (inert flag), `color` (kanban-only).

Indexes: `unique(token_nonce) WHERE deleted_at IS NULL`;
`(survey_id, invite_token)`; `(survey_id, partner_id)`; `(survey_id, email)`;
`(survey_id, state)` for leaderboard/session scans; `(wire_identity_key)`.

### 2.5 UserInputLine (the answer line — polymorphic value)

| Column | Type | Constraints / notes |
|---|---|---|
| id | uuid PK | |
| user_input_id | uuid NOT NULL | FK cascade, indexed |
| survey_id | uuid | denormalized from the input (upstream stored-related; kept stored for the cross-facet reads) |
| question_id | uuid NOT NULL | FK cascade, indexed |
| suggested_answer_id | uuid? | the chosen answer (`answer_type = suggestion`) |
| matrix_row_id | uuid? | the matrix row discriminator (same comodel) |
| skipped | boolean | the skip marker; service invariant `skipped ⟺ answer_type IS NULL` (SV-C5 stays service-level: the typed-value-presence XOR with the float/scale zero-exemptions is intake logic, not expressible as a column CHECK) |
| answer_type | enum `survey_answer_type`? | selects the value column; the upstream duplicate "Number" display label does not port (both render labels are the webapp's) |
| value_char_box | string? | ALSO the storage for comments (always `char_box`) |
| value_text_box | text? | |
| value_numerical_box | float? | legitimate 0.0 passes intake (zero-exemption kept) |
| value_scale | integer? | |
| value_date | date? / value_datetime datetime? | |
| answer_score | float? | **written once at submit; immutable outside `regrade`** (§5.4) |
| answer_is_correct | boolean? | same immutability |
| speed_seconds | integer? | **NEW (recorded addition): the elapsed-seconds basis captured at submit** — the immutable speed snapshot that makes regrade reproducible and drift detectable (SV-B2 fix) |
| answered_at | datetime | NOT NULL default now — capture time |

Index: `(user_input_id, question_id)`.

### 2.6 Attempt-pool counting (verbatim port)

Pool identity (raw-SQL self-join in the repository, `user_input.model.yaml` service notes
`:137-150`): same survey + `state = done` + `test_entry IS NOT TRUE` + (`invite_token`
shared or both NULL) + (same `partner_id` OR same `email`). `_has_attempts_left` and the
first-success publication rule (§7) read this pool. Flushed inside the caller's
transaction so pending writes are visible.

## 3. Lifecycles (ADR-0016 declarations)

| Field | Shape | Machine | Notes |
|---|---|---|---|
| `UserInput.state` | `hand_set` | `survey_input_state` | `new → in_progress → done`, exactly three values, **no `skipped`** (skipping is per-line), no label inversion. Writers: `begin` verb (+start_datetime), `mark_done` funnel (+end_datetime), session-end bulk-done. Session attendees may be created directly `in_progress` when `session_state = in_progress` (T0). **Plus the DB monotonic guard (§6) — deliberate strengthening beyond upstream's controller-only check** (register row SVM-9) |
| `Survey.session_state` | `hand_set` | `survey_session_state` | NULL → `ready` (arm verb, forces `page_per_question`) → `in_progress` (lazy open on first advance) → NULL (end verb). Hand-set only; `no_cancelled`-style extra values are not invented |
| `Survey` (the entity) | `none` | — | archive/temporal only — **there is no survey state field** (SVM-1). Declared `lifecycle: none` so lint catches any smuggled status enum |
| `Question.question_type` | clamp guard (`hybrid`-family write guard) | — | pages forced NULL; the user's choice persists until a guard clamps it (SVM-2 semantics: recompute = clamp, not derive) |

The full editable-compute guard cluster (SVM-2) ports as **write-path clamps in the
survey/question update verbs** — never as silent derivations: `scoring_type`,
`certification`, `certification_give_badge`, `is_attempts_limited`, `session_code`
(generation), `question_type`, `is_scored_question`, `save_as_email`, `save_as_nickname`,
`validation_required`, `page_id`.

**SV-B3 fixed structurally + asserted:** there is no early-return write path — a single
update touching both certification flags and speed-rating settings applies ALL clamps and
persists ALL propagated fields (`session_speed_rating_time_limit` → questions) in one
transaction; probe 12 covers the combined write.

## 4. Enums (public schema, collision-checked)

Thirteen enum types, created UNQUALIFIED in `public` (module convention). The collision
census (2026-08-31) enumerated every `CREATE TYPE` across all module migrations in the
metaphora tree (~450 names, including the W6 siblings' `badge_grant_kind`,
`challenge_state`, `goal_state`, `mailing_state`, `trace_status`, `rating`-adjacent sets).
**No proposed name collides.** Nearest neighbors verified distinct:
`session_status`/`pos_session_status` (≠ `survey_session_state`), `mailing_state` (≠
`survey_input_state`), `item_type`/`task_category` (no `question_type`/`answer_type`
exists anywhere).

| # | Enum name | Values |
|---|---|---|
| 1 | `survey_survey_type` | `survey`, `live_session`, `assessment`, `custom` |
| 2 | `survey_access_mode` | `public`, `token` |
| 3 | `survey_scoring_type` | `no_scoring`, `scoring_with_answers_after_page`, `scoring_with_answers`, `scoring_without_answers` |
| 4 | `survey_session_state` | `ready`, `in_progress` — display labels `Ready` / `In Progress`; NULL = none/ended |
| 5 | `survey_questions_layout` | `page_per_question`, `page_per_section`, `one_page` |
| 6 | `survey_questions_selection` | `all`, `random` |
| 7 | `survey_report_layout` | `modern_purple`, `modern_blue`, `modern_gold`, `classic_purple`, `classic_blue`, `classic_gold` |
| 8 | `survey_question_type` | `simple_choice`, `multiple_choice`, `text_box`, `char_box`, `numerical_box`, `scale`, `date`, `datetime`, `matrix` |
| 9 | `survey_matrix_subtype` | `simple`, `multiple` |
| 10 | `survey_input_state` | `new`, `in_progress`, `done` — display labels `New` / `In Progress` / `Completed`, NO inversion (machine value = contract, labels are webapp data per ADR-0016 `display_labels`) |
| 11 | `survey_answer_type` | `text_box`, `char_box`, `numerical_box`, `scale`, `date`, `datetime`, `suggestion` — the upstream duplicate "Number" label on numerical_box AND scale is dropped (render labels are the webapp's; values stay distinct) |
| 12 | `survey_invite_existing_mode` | `new`, `resend` |
| 13 | `survey_progression_mode` | `percent`, `number` (kept as data for the webapp progress bar) |

Council condition 13: the boot checklist includes the **recorded-vs-applied enum census** —
the test plan's census probe diffs these thirteen names (+ values) against `pg_enum` in the
scratch DB after migration, catching the first-created-wins collision class.

## 5. Security: the token-capability model

Zero portal/public grants, explicit world-deny posture preserved (SVF; 23-row ACL table →
guarded routes only, participants never touch the ORM). Every attempt is gated by tokens
plus the anti-cheat windows. Re-declared on ADR-0018 tiers:

### 5.1 Tier A — the per-attempt machine capability (replaces `user_input.access_token`)

The upstream never-expiring, never-rotating uuid4 is the ADR-0018-named non-conformance;
the port uses the engagement rating shape verbatim:

- Columns on the input row: `token_nonce` (128-bit hex selector, partial UNIQUE among live
  rows) + `token_expires_at` (mandatory; default TTL 30 d, config-overridable).
- The public link/path carries `{input_id}.{nonce}.{exp}.{mac}`; the MAC is HMAC-SHA256
  over `(id, nonce, grant, exp)` with a server secret (env-var reference), constant-time
  compared. A leaked row alone is inert — the nonce is not the capability.
- **Multi-use within the attempt** (unlike rating's single-use submit): an attempt spans
  many submits; the token authorizes `begin`/`submit`/`next`/`certification` reads until
  `state = done` or expiry. Terminal states refuse with the shared shape below.
- **Rotation:** the guarded `rotate_token` verb (officer) and every invitation resend mint
  a fresh nonce + expiry; the old nonce dies atomically.
- Verification endpoints are throttled (120/min per client, the rating-route setting) and
  answer a **shared refusal** — unknown, expired, malformed, and wrong-state are
  indistinguishable (`not_submittable`-class body); no enumeration oracle (ADR-0019 §3).
- Expiry refusal is a typed `410`; the clock is UTC everywhere (SV-B1).

The **survey-level `access_token` stays a plain uuid** (unique, public URL key): it is
public infrastructure, not a credential — participants are authorized by the per-attempt
capability, never by the survey token (SVF).

### 5.2 Tier B — the session code (human-typed)

`/survey/s/:code` join surface. The code is short by UX necessity, so the other three ADR-0018
knobs are all present (attendance kiosk-PIN precedent, `attendance_write_service.rs:32-70`):

- **Expiry:** codes are valid only while `session_state IS NOT NULL`; the end verb and a
  hard TTL (24 h after arm) invalidate.
- **Generation:** length grows 4→9 digits on collision; **exhaustion at 9 digits fails
  loudly** (`SessionCodeExhausted` typed error — SV-B12 fixed; the upstream False-emitting
  generator does not port). Uniqueness is the DB partial UNIQUE (SV-S2).
- **Attempt counters + escalating lockout:** per-identity AND per-IP; 3 consecutive
  failures → 30 s lock, doubling per extra failure, 15 min cap, 1 s minimum spacing.
  Constants live as named `pub const`s with pure `lockout_until()` (probe 2 asserts the
  pure function). Success resets the counter.
- **Route throttle:** the code-check/join routes ride the same 120/min per-client
  middleware plus the counters; the shared-refusal rule applies (wrong code and unknown
  code are the same answer).

### 5.3 The frozen scoring denominator

- `create()` of an input snapshots `predefined_question_ids` from the survey
  (`random.sample` per section when `questions_selection = random`) — TR-SV-5 verbatim.
  Later survey edits NEVER change existing scores; the scoring service reads ONLY the
  snapshot.
- `mark_done` prunes inactive conditional questions from the snapshot (funnel step 4).
- DoD probe 4: mid-attempt question-set edits leave a computed score byte-identical.

### 5.4 The speed-rating engine + the LOUD drift refusal (SV-B2)

Formulas (SVM-11, ported verbatim), evaluated at submit inside the intake transaction:

| elapsed (against the STORED `session_question_start_time`, written `now+1s`) | score |
|---|---|
| `< 2 s` | 100 % of the line's points |
| `> question.time_limit` OR the line's question ≠ the live cursor | exactly 50 % |
| between | `points/2 × (1 + (limit − secs)/(limit − 2))` — 50 % floor + linear decay |

Eligibility: positive raw score ∧ `is_session_answer` ∧ survey `session_speed_rating` ∧
time-limited question. The pushed realtime payload carries the **pre-write** millis
(client display basis); the graded basis is the stored `now+1s` clock — attendee-favoring,
kept from upstream. Correct answers always keep ≥ 50 %.

**The drift trap, fixed loudly (register row SV-B2 "fixed loudly" mandate):**

1. `answer_score`, `answer_is_correct`, `speed_seconds`, and the value columns are written
   **once** by the intake path, inside the submit transaction, with `speed_seconds`
   captured as the elapsed basis.
2. Any later write that would alter `answer_score`/`answer_is_correct`/`speed_seconds` on
   an existing line — service path OR raw SQL — is refused. Service side: the typed error
   `SurveyWriteError::ScoreDriftRefused { line_id, attempt_no }` (never a silent
   overwrite, never a re-derivation from a fresh `now()`). DB side: the hand hardening
   migration adds a BEFORE UPDATE trigger raising `survey_score_drift_refused` when
   `OLD.answered_at IS NOT NULL AND (NEW.answer_score, NEW.answer_is_correct,
   NEW.speed_seconds) IS DISTINCT FROM (OLD.…)` outside the regrade marker (§6.1).
3. The ONLY sanctioned recompute is the guarded `regrade_question` verb: it recomputes
   from the STORED `speed_seconds` and value columns (never from a wall clock), records
   the old/new pair for audit, and sets the regrade marker the trigger checks.

Named regression probe 5 asserts: post-submit value edits on another column do not move
the score; a forced score rewrite hits the typed error; regrade reproduces the original
basis from `speed_seconds`.

### 5.5 The intake contract (TR-SV-8, ported verbatim)

Per question on `submit`: (1) validity — `state = done` refuses (re-entry guard, now
backed by the monotonic trigger); (2) anti-cheat — `_has_attempts_left` (pool count) +
time-limit graces **+10 s survey / +3 s question** against the stored clocks (probe 16
asserts the boundaries); (3) dispatch — skip inactive conditionals → validate (ranges,
email, mandatory×roaming) → save lines; (4) validation errors suppressed when a time
limit was reached; (5) clear inactive conditional answers (delete dependent lines —
scoring correctness over UX, kept); (6) reveal flags per `scoring_type`; (7) terminal
`mark_done` / next-page / skipped-repair loop.

Storage semantics (`_save_lines` family, verbatim): scalar types UPSERT in place; choice
and matrix DELETE-AND-RECREATE; empty choice answer materializes a **skipped line**
(the `[False]` marker's observable effect); comments ALWAYS `char_box` +
`value_char_box`; `overwrite_existing` gated on `users_can_go_back`/`save_as_*` (refused
loudly otherwise). The `save_as_email`/`save_as_nickname` INPUT-row side-write is the one
leg of this contract that landed as a repository but stays **unwired** —
`set_identity_side_writes` (`attempt_repository.rs`) has no caller, no submit path stamps
the input row from a `save_as_*` answer, and probe 7 asserts none of it (a recorded
follow-up, not a shipped behavior).

### 5.6 The completion funnel (`mark_done`, TR-SV-4 with the batch-abort fixed)

Per input, in order: write `state = done` + `end_datetime` (monotonic-safe conditional
UPDATE) → completion fact (host-relayed; the follower-note subtype becomes an event) →
certification leg (§7) → prune the snapshot. **Per-input error isolation (register row
"Batch-abort in _mark_done"):** each input's funnel is independent — a failure on input N
is recorded (typed, audited) and the batch continues; state already written stays
written. Upstream's abort-on-first-mail-failure does not port.

## 6. DB-level monotonic attempt guard (condition 12 strengthening — shape chosen)

**Chosen shape: a BEFORE UPDATE trigger on `survey.user_inputs`** (plus
state-guarded conditional UPDATEs as the service's first line).

- Trigger logic: when `NEW.state IS DISTINCT FROM OLD.state` and
  `rank(NEW.state) < rank(OLD.state)` (rank: `new=1, in_progress=2, done=3`), raise
  `survey_input_state_not_monotonic`. Covers raw SQL, the session-end bulk write, batch
  tools, and any future code path — which is the point: upstream's ONLY protection was a
  controller check (`main.py:534`), and ADR-0015/SM-B6 house rules put monotonic guards at
  the DB.
- **Why not a partial UNIQUE:** a partial unique expresses "at most one row per key"
  (exactly-right for grant-key idempotency in engagement, and for live-token uniqueness
  here) — it cannot express a per-row directional transition, and a CHECK constraint
  cannot reference the prior row. The trigger is the only DB-level shape that expresses
  "the value may only advance".
- Service first line: transition verbs are conditional UPDATEs
  (`... WHERE id = $1 AND state = $expected` returning rows; zero rows → typed
  `StateConflict`), so the common path fails fast with a precise error; the trigger is
  the backstop for everything else.
- `end_datetime` is stamped only on the `→ done` edge by the same conditional UPDATE, so
  a double `mark_done` is a no-op refusal rather than a timestamp rewrite.

### 6.1 Hand-written DB objects (the hardening stamp)

The DSL-expressible guards ride the codegen CHECK constraints (§2); the hand stamp
carries what it cannot express, each with a `user_owned:` migration glob:

- `survey_input_monotonic_guard` trigger (+ the score-drift refusal trigger on
  `survey_user_input_lines`, §5.4, sharing the regrade-marker mechanism).
- Question deletion block during live sessions (SV-D1): refuse DELETE of a survey's
  questions while `session_state = 'in_progress'` — an INSTEAD OF/BEFORE DELETE trigger
  (upstream was ORM-only; a raw DELETE mid-session corrupts the running cursor, and the
  house rule is DB-level).
- The attempt-pool functional index and leaderboard covering indexes.

## 7. Certification: the event contract + the fail-closed port

Upstream wiring (create-time gamification triple, `_cron_update(commit=False)` inline,
officer badge CRUD) does not port — council condition 7 banned survey-facing badge CRUD,
and EN-88 replaced cron-running with the event path. The contract is **already pinned by
the consumer**; this module is the producer.

### 7.1 The event

`CertificationPassed` — declared in engagement's `schema/hooks/index.hook.yaml:58-83`;
the module emits it through the port below (the host adapter delivers in-process to
`gamification_write_service.rs:784-817 on_certification_passed`).

| Field | Type | Survey-side derivation |
|---|---|---|
| `certification_ref` | string | `"survey:{survey_id}"` — stable, namespaced |
| `survey_ref` | uuid? | the survey id |
| `attempt_ref` | string | the winning `UserInput.id` (uuid string) |
| `recipient_user_id` | uuid | the input's `user_id` (guarded non-null below) |
| `badge_key` | string | the survey's `certification_badge_key` |

**Idempotency key** (derived by the consumer, restated here as the contract):
`event:certification:{certification_ref}:{attempt_ref}` — exactly-once per (survey,
attempt) under at-least-once delivery, via engagement's `grant_key` partial UNIQUE (R-G1).

**Once-per-user semantics are producer-side:** the funnel publishes ONLY on the **first
successful completion in the attempt pool** (the §2.6 self-join: same survey + done + not
test + shared-or-null invite token + same partner-or-email, no prior
`scoring_success = true` member). Later passing retries create their attempts and score
records but do not re-publish — preserving Odoo's challenge `period = 'once'` behavior
under engagement's (certification, attempt) key. Recorded as the producer's ruling.

**Publication gate:** `certification ∧ certification_give_badge ∧ scoring_success ∧
user_id IS NOT NULL ∧ NOT test_entry`. The `certification_give_badge` clamp (login
required) makes `user_id` normally guaranteed; a NULL `user_id` at the gate is a config
violation — the fact is NOT published, a typed audit record is written, and the funnel
continues (per-input isolation; never a silent drop).

### 7.2 The fail-closed port trait (PhoneBookPort precedent)

```rust
// src/application/service/certification_port.rs (user-owned)
#[async_trait]
pub trait CertificationGrantPort: Send + Sync {
    /// Deliver a CertificationPassed fact. Implementations MUST be idempotent
    /// under redelivery (the consumer's grant_key dedups).
    async fn certification_passed(&self, fact: CertificationFact) -> Result<(), CertificationGrantError>;
}

#[derive(Debug, thiserror::Error)]
pub enum CertificationGrantError {
    /// No port composed — the deny-by-default refusal (fail closed).
    #[error("certification grant not composed: {detail}")]
    NotComposed { detail: String },
    #[error("delivery failed: {0}")]
    Delivery(String),
}

pub struct RefusingCertificationGrant; // ships as the default install
```

- Install: `SurveyModule::set_certification_grant(Arc<dyn CertificationGrantPort>)`
  (mirrors `MessagingModule::set_phone_book`). Unwired ⇒ every certification completion
  fails LOUDLY with `NotComposed` (audited, isolated per §5.6) — never a silent no-grant.
- ZERO Cargo edge onto backbone-engagement: the host wires the adapter that calls
  engagement's public `on_certification_passed`. The badge is referenced only by its
  stable key string (`certification_badge_key`); survey performs no badge CRUD, no karma
  writes, no challenge reads.
- The publication runs **post-commit** of the input's `mark_done` transaction (a grant
  must never ride a rollback); redelivery safety is the consumer's grant_key.

### 7.3 Outbound facts (host-relayed, D-17)

`SurveyEventSink` (same trait/refusing-default pattern, default `TracingEventSink`):
`answer_completed`, `participant_invited` (per recipient, carrying the Tier A link for the
host's mail relay), `session_started` / `session_advanced` / `session_ended` (the realtime
legs, §8). The module sends no mail itself.

## 8. Realtime: the sanctioned composition (condition 11)

**The Odoo shape (survey token as bus channel) does NOT port as-is** — mail's SSE
allowlist drops non-record channels by construction (`sse.rs:101-106`; the bare token
string is not a parseable `{model}_{id}` key). The recorded ADR-0017 deviation:

- **Channel (record-shaped):** `survey.survey_{survey_id}` — built with the mail
  grammar (`constants.rs:38 record_channel`), parseable by `tailer.rs:60-64`.
- **Payload contract** (the sink stages these onto `messaging.outbox_events` with the
  channel key; `message.type` + payload):
  - `next_question`: `{ question_start_ms: <pre-write now with millis>, question_id, sequence }`
  - `end_session`: `{}`
  The stored clock is written `now + 1 s` in the same transaction (server-delay grace,
  kept from TR-SV-6).
- **Who streams:** attendees use mail's existing `GET /mail/realtime/stream` — survey
  adds NO SSE route of its own.
- **What the host resolver must answer:** the host installs a `ThreadAccessResolver`
  (trait at `chatter_acl.rs:75`) whose `can_read(identity, "survey.survey", survey_id)`
  returns true iff a live row exists in `survey.survey_user_inputs` with
  `survey_id = res_id AND wire_identity_key = <identity handle> AND state <> 'done'` and
  the survey's `session_state IS NOT NULL`. `can_post` returns false (attendees never
  write through chatter). The `wire_identity_key` column is stamped at join/begin by the
  public routes, which run behind the host-mounted guest middleware (below).
- **Guest mint:** anonymous attendees get a `mail.guest` row + `dgid` cookie via mail's
  existing `POST /mail/guest` flow mounted on the join path (host composition; the survey
  module stores only the opaque handle). Users identify by session; their resolver key is
  the user id.
- **Host compose step (condition 11, named):** `"survey realtime resolver + guest mint"`
  — one compose block in `apps/serpa-service` that (a) mounts the guest middleware over
  the survey public family, (b) installs the resolver above (the host's FIRST-ever
  `ThreadAccessResolver` install), (c) wires the outbox-backed `SurveyEventSink`. **Its
  own probe (§12 #15):** a staged `next_question` event reaches a stub identity whose
  `wire_identity_key` matches a live input, and is dropped for a foreign/done one.
- **SV-B4 fixed:** the advance verb takes `SELECT ... FOR UPDATE` on the survey row (or
  an equivalent conditional UPDATE on the cursor) — the read-modify-write race is closed;
  the +50 % compensation branch for late answers is still evaluated per formula. Probe 9
  hammers two concurrent advances.
- **SV-B6 kept as a documented seam:** session-manage reads are fenced (officer
  visibility), the advance write is a service verb — the asymmetry is upstream-documented
  and recorded, not "fixed" away.

**No survey-owned outbox schema** and therefore no host `outbox_schemas` change: session
events stage into `messaging` (already in the drain list — this deliberately avoids the
condition-2 overlay class entirely).

## 9. Route surface

Two public-route classes exist in-tree (council condition 11): (1) HMAC-verified webhook
mounts (mail/payment-gateway), (2) **bare capability mounts, throttled, shared-refusal**
(engagement `/r/:code`, `/rate/:token/submit`). Survey uses **class 2 only**. The
AuthContext-reading gate class (DIT #229) is NOT used.

### 9.1 Guarded (host nests at `/api/v1/survey`, behind identity + the module-write gate)

| Method | Path | Verb |
|---|---|---|
| CRUD | `/surveys`, `/questions`, `/question-answers` | generated 12-endpoint handlers (officer compose) |
| GET | `/user-inputs`, `/user-input-lines` | generated read-only (writes only via verbs) |
| POST | `/surveys/:id/invite` | invitation batch (partners + free-form emails split `[;,\n\r]+`, validated, deduped; `existing_mode` new/resend; deadline propagation; mints inputs + Tier A links; emits `participant_invited` facts) |
| POST | `/surveys/:id/test-start` | test-mode entry (`test_entry = true`) |
| POST | `/surveys/:id/session/start` | arm: clamp layout, `session_code` mint, `session_state = ready` |
| POST | `/surveys/:id/session/next` | advance (FOR UPDATE; clock + push, §8) |
| POST | `/surveys/:id/session/end` | end: bulk-done attendees (forward-only), push `end_session` |
| GET | `/surveys/:id/session/leaderboard` | top-15 by raw `scoring_total` since `session_start_time`, current-question score subtracted; nickname identity |
| GET | `/surveys/:id/statistics` | per-section correct/partial/incorrect/skipped (done + not-test) |
| POST | `/user-inputs/:id/rotate-token` | Tier A rotation |
| POST | `/user-inputs/:id/resend` | pool re-entry (fresh attempt, same invite_token) |
| POST | `/surveys/:id/questions/:qid/regrade` | the sanctioned score recompute (§5.4) |

### 9.2 Public (bare mount at root, throttled 120/min per client; Tier B counters on code routes)

| Method | Path | Notes |
|---|---|---|
| GET | `/survey/s/:code` | session metadata preview (safe: no writes — ADR-0019 §1) |
| POST | `/survey/s/:code/join` | Tier B verify (counters/lockout) → attempt pre-creation (THE anti-cheat: the row exists before any submit) + Tier A mint + guest-handle stamp |
| POST | `/survey/start/:survey_token` | public (non-session) entry: intake gate (`active`, access_mode, attempts left, deadline UTC) → attempt pre-creation + Tier A mint |
| POST | `/survey/attempt/:token/begin` | `new → in_progress` (+start_datetime) + first page/question payload |
| POST | `/survey/attempt/:token/submit` | THE intake (§5.5) |
| GET | `/survey/attempt/:token/next` | live-session attendee fetch (safe read; polling) |
| GET | `/survey/attempt/:token/certification` | scoring evidence for the webapp certification page (safe; requires a `scoring_success` input — §10) |

All public mutators are POST (no mutating GETs); all refusals share one body shape per
route family (no oracle). Route factories on the module:
`pub fn routes(self: &Arc<Self>) -> Router` (guarded composer, §9.1),
`pub fn public_composer() -> Router<ApiState>` (§9.2, throttle layer inside — the
`rating_routes.rs public_composer` shape).

## 10. The four condition-12 dispositions (recorded BEFORE the certification leg)

1. **auth_signup substitution.** Mid-survey signup URLs do not port. The headless
   substitution is the token-gated API itself: the per-attempt Tier A capability IS the
   participant identity for the attempt's lifetime; partner/email association happens at
   invite time or via `save_as_email`; login-required surveys authenticate through the
   host's standard session and stamp `user_id`. The `users_can_signup` config read is
   dropped. (Council-decided: survey's `http_routing`/`auth_signup` deps collapse into
   the webapp + token-gated API.)
2. **Certification PDF.** No PDF-rendering surface exists platform-wide (register row).
   Disposition: the report action + paperformat do not port; the webapp renders a
   token-gated certification page from `GET /survey/attempt/:token/certification`
   (survey title, score/percentage/success, dates, badge key, `certification_report_layout`
   as the style selector); browser print is the paper path. The certification MAIL becomes
   the host-relayed completion fact (§7.3) — attachment-bearing mail is a document-rendering
   seam filed for whenever one lands. Recorded, not silently dropped.
3. **Session-end bulk-done bypass + per-input error isolation.** Kept faithful with one
   structural improvement: the end verb bulk-moves attendees to `done` via the
   monotonic-safe conditional UPDATE (forward-only — passes the guard by construction) and
   deliberately does NOT run the certification funnel, matching upstream TR-SV-7. **The
   bypass can never skip a certification:** `arm_session` refuses certification-bearing
   surveys with the typed `SessionCertificationConflict` BEFORE any session code is
   minted (`survey_write_service.rs`), and a database CHECK
   (`survey_session_certification_disjoint`, migration
   `20260831000003`) stamps the same disjointness against every other writer — a
   badge-bearing survey cannot run a live session (probe 10b asserts the refusal, the
   control arm, and the CHECK's 23514). Per-input error isolation in the normal funnel is
   §5.6.
4. **DB-level monotonic attempt guard.** Deliberate strengthening beyond Odoo (upstream:
   controller check only). Shape = trigger, justified in §6; recorded here as the
   condition-12 disposition and as register row SVM-9's answer.

## 11. Zero-cron declaration

`scheduled_jobs: []` in the module hooks — no ir.cron equivalent, no autovacuum, no
config parameters. Badge granting is the post-commit event publish (§7); session advance
is verb-driven push + attendee polling (§8); expiry/lazy completion stays read-path
(`done` on next page view past the survey time limit — T3 kept). Audit config:
`retention_days: 2555` (the spec source's certification/compliance weight), critical
events: `survey_answer_started`, `survey_answer_completed`, `survey_certification_grant_refused`
(the fail-closed/no-user observables), `survey_session_advanced`, `survey_token_rotated`,
`survey_code_verify_failed`.

## 12. Migration plan

Module migration bookkeeping is `schema_migrations (module, name)` via the metaphor
runner — **never bare `sqlx migrate` against serpa databases** (condition 13). The
scratch test Postgres is the docker container `payroll-p5-testdb` on `127.0.0.1:5433`
(start if down; create/drop a dedicated scratch DB per suite run; never point tests at
5432).

| Stamp | Origin | Content |
|---|---|---|
| `NNNN_create_enums` | codegen | the 13 public enums, `IF NOT EXISTS`-guarded (§4) |
| `NNNN_create_survey_table` … one per entity + join tables (`restrict_users`, `langs`, `triggering_answers`, `predefined_questions`) | codegen | tables, FKs, DSL-expressible CHECKs (§2) |
| `NNNN_add_audit_triggers` | codegen | house audit |
| `NNNN_survey_input_monotonic_guard` | **hand** (`user_owned` glob `migrations/*monotonic*`) | the state trigger + regrade-marker score-drift trigger (§6/§5.4) |
| `NNNN_survey_hardening_constraints` | **hand** (`migrations/*hardening*`) | XOR checks the generator cannot order (page/type, answer-role), the live-question delete block, partial uniques (badge key, token nonce), pool/leaderboard indexes |

Codegen-owned files are regenerated freely; every hand file lands under a `user_owned:`
glob in `metaphor.codegen.yaml` BEFORE it is written (the declaration-before-landing
contract; the P2 council removed the dangling-declaration class).

## 13. Test plan (fail-hard probes — fresh scratch DB per case, `tests/behavior/common` harness, `tower::ServiceExt::oneshot` for routes)

| # | Probe | Asserts |
|---|---|---|
| 1 | `token_tier_a_lifecycle` | expiry → typed 410; rotated nonce → old dies atomically; shared refusal shape indistinguishable across unknown/expired/malformed/done |
| 2 | `tier_b_code_lockout` | pure `lockout_until()` table (3→30 s, doubling, 15 min cap); counters per identity AND IP; success resets; exhausted generator raises `SessionCodeExhausted` (SV-B12) |
| 3 | `attempt_monotonic_guard` | service regression refused; **raw SQL** `done → in_progress` refused by trigger; forward edges pass; session-end bulk write passes |
| 4 | `denominator_frozen` | mid-attempt question add/remove/score-edit → recomputed score byte-identical (snapshot only) |
| 5 | `speed_snapshot_immutable` | the SV-B2 regression BY NAME: post-submit edits never move `answer_score`; forced rewrite → `ScoreDriftRefused`; `regrade` reproduces from stored `speed_seconds` |
| 6 | `speed_formula_table` | pure engine table: 0 s/1.9 s → 100 %, mid-window linear value, over-limit → 50 %, superseded question → 50 %, floor never below 50 % for correct |
| 7 | `intake_contract` | upsert / delete-recreate / skipped-line materialization / comments-as-char_box / overwrite refusal without roaming (the `save_as_*` side-writes stay unwired — not asserted; see §5.5) |
| 8 | `conditional_edge` | unselected trigger → dependent lines deleted at submit; random selection ignores conditionals; earlier-sequence domain enforced |
| 9 | `session_advance_lock` | two concurrent advances → exactly one cursor move, one clock stamp, one push (SV-B4) |
| 10 | `session_end_bulk_done` | attendees forward-done; no certification publication; code dies with the session; advance/end refuse re-entry |
| 10b | `certification_arm_refused` | certification survey arms a session → typed `SessionCertificationConflict`, no code minted; plain survey arms as control; direct UPDATE trips the CHECK (SQLSTATE 23514) — the §10.3 disjointness |
| 11 | `certification_exactly_once` | first pool success publishes once; duplicate delivery → one grant (grant_key); later passing retries do not re-publish; NULL `user_id` → audited refusal, funnel continues |
| 12 | `certification_port_fail_closed` | unwired port → `NotComposed`, loud, isolated per-input; combined certification+speed-rating update syncs both (SV-B3) |
| 13 | `enum_census` | the 13 recorded names + value sets match `pg_enum` in the scratch DB post-migration (condition 13) |
| 14 | `realtime_channel_shape` | channel grammar `survey.survey_{id}` parses; resolver allow for matching live `wire_identity_key`, deny for foreign/done; guest-handle stamp at join |
| 15 | `resolver_compose_probe` | the condition-11 named compose probe: staged `next_question` reaches the allowed stub identity and is dropped for the disallowed one |
| 16 | `anti_cheat_windows` | +10 s survey / +3 s question grace boundaries; attempt pre-creation precedes any submit; pool counting SQL (shared-or-NULL invite token, partner-or-email) |
| 17 | `public_surface_hygiene` | GETs side-effect-free (row snapshots unchanged); throttle arms on the public family; world-deny (no anonymous read of guarded routes) |
| 18 | `deadline_utc` | deadline boundaries correct across non-UTC server TZ simulation (SV-B1) |
| 19 | `finish_prunes_unfired_conditionals` | the terminal-edge prune: conditional whose trigger never fired leaves the frozen set at finish (B-path 10/20 = 50 %; answered-anyway conditional stays → 100 %; unanswered NON-conditionals stay → honest 0 %) — §5.3 step 4 |

Zero-cron assertion rides the hooks lint (`scheduled_jobs: []`) plus the wave-close
hygiene pass.

## 14. Register row answers (audit map)

Every `backbone-survey` row in `docs/plan/w6-register-deltas.md` (24) is answered by a
section here: SVM-1 §2.1/§3; SVM-2 §3; SVM-5 §2.2; SVM-7 §2.2/§5.5; SVM-8 §2.2; SVM-9
§3/§6; SVM-10 §2.4 + read service; SVM-11 §5.4; SV-B1 §2.4/§5.1; SV-B2 §5.4; SV-B3 §3
(probe 12); SV-B4 §8 (probe 9); SV-B5 §2.1; SV-B6 §8; SV-B12 §5.2 (probe 2); SVF
token-capability §5; intake contract §5.5; invite_token pool §2.6; frozen denominator
§5.3; realtime shape §8; certification PDF §10.2; auth_signup §10.1; session-end bypass
§10.3; batch-abort §5.6.

## 15. `metaphor.codegen.yaml` — declared before landing

```yaml
user_owned:
  # hand-authored write paths
  - "src/application/service/survey_write_service.rs"      # clamp guards, session verbs, advance lock
  - "src/application/service/intake_service.rs"            # begin/submit, intake contract, funnel
  - "src/application/service/scoring_service.rs"           # frozen denominator, speed engine, regrade
  - "src/application/service/attempt_service.rs"           # entry/join, Tier A mint/rotate, pool counting
  - "src/application/service/certification_port.rs"        # fail-closed port + refusing default
  - "src/application/service/event_sink.rs"                # SurveyEventSink + tracing default
  - "src/application/service/session_read_service.rs"      # leaderboard, statistics, most-voted
  # hand-authored SQL holders
  - "src/infrastructure/persistence/survey_session_repository.rs"
  - "src/infrastructure/persistence/attempt_repository.rs"
  - "src/infrastructure/persistence/scoring_repository.rs"
  # public + guarded route groups
  - "src/presentation/http/public_routes.rs"
  # hand-written DB hardening
  - "migrations/*monotonic*"
  - "migrations/*hardening*"
  # behavior tests + docs
  - "tests/**"
  - "docs/**"
  - "README.md"
  - "SPEC.md"
```

(Exact file names may grow at implementation; the contract is declaration-before-landing
for every hand file inside generator-owned trees.)
