//! Database schema — table definitions applied on startup via `CREATE TABLE IF NOT EXISTS`.
//!
//! All tables use UUIDs as primary keys.  Timestamps are stored with timezone
//! (`TIMESTAMPTZ`).  JSONB is used for flexible metadata columns.

/// SQL to create (or ensure existence of) all Kainetic Cloud tables.
///
/// Safe to run on every startup: every statement uses `IF NOT EXISTS`.
pub const SCHEMA_SQL: &str = r#"
-- ── Users ──────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS kc_users (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    email       TEXT        UNIQUE NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Teams ──────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS kc_teams (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT        NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Team membership + RBAC ─────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS kc_team_members (
    team_id     UUID        NOT NULL REFERENCES kc_teams(id) ON DELETE CASCADE,
    user_id     UUID        NOT NULL REFERENCES kc_users(id) ON DELETE CASCADE,
    role        TEXT        NOT NULL CHECK (role IN ('viewer', 'developer', 'admin')),
    joined_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (team_id, user_id)
);

-- ── API keys ───────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS kc_api_keys (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    team_id     UUID        NOT NULL REFERENCES kc_teams(id) ON DELETE CASCADE,
    name        TEXT        NOT NULL,
    key_hash    TEXT        NOT NULL,
    prefix      TEXT        NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ
);

-- ── Registered agents ──────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS kc_agents (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    team_id     UUID        NOT NULL REFERENCES kc_teams(id) ON DELETE CASCADE,
    name        TEXT        NOT NULL,
    version     TEXT        NOT NULL DEFAULT '0.1.0',
    description TEXT,
    config      JSONB       NOT NULL DEFAULT '{}',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (team_id, name, version)
);

-- ── Deployments ────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS kc_deployments (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    team_id     UUID        NOT NULL REFERENCES kc_teams(id) ON DELETE CASCADE,
    agent_id    UUID        NOT NULL REFERENCES kc_agents(id),
    status      TEXT        NOT NULL CHECK (status IN ('pending','running','stopped','failed')),
    url         TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Runs ───────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS kc_runs (
    id                  UUID        PRIMARY KEY,
    team_id             UUID        NOT NULL REFERENCES kc_teams(id) ON DELETE CASCADE,
    agent_id            UUID        REFERENCES kc_agents(id),
    agent_name          TEXT        NOT NULL,
    status              TEXT        NOT NULL CHECK (status IN ('running','completed','failed','cancelled')),
    input_preview       TEXT,
    output_preview      TEXT,
    error_message       TEXT,
    prompt_tokens       INTEGER     NOT NULL DEFAULT 0,
    completion_tokens   INTEGER     NOT NULL DEFAULT 0,
    total_cost_usd      DOUBLE PRECISION NOT NULL DEFAULT 0,
    duration_ms         INTEGER,
    started_at          TIMESTAMPTZ NOT NULL,
    completed_at        TIMESTAMPTZ,
    metadata            JSONB       NOT NULL DEFAULT '{}'
);

-- ── Spans (fine-grained trace events per run) ──────────────────────────────────
CREATE TABLE IF NOT EXISTS kc_spans (
    id              UUID        PRIMARY KEY,
    run_id          UUID        NOT NULL REFERENCES kc_runs(id) ON DELETE CASCADE,
    team_id         UUID        NOT NULL REFERENCES kc_teams(id) ON DELETE CASCADE,
    parent_span_id  UUID,
    name            TEXT        NOT NULL,
    kind            TEXT        NOT NULL DEFAULT 'internal',
    status          TEXT        NOT NULL DEFAULT 'ok',
    start_time      TIMESTAMPTZ NOT NULL,
    end_time        TIMESTAMPTZ,
    attributes      JSONB       NOT NULL DEFAULT '{}',
    events          JSONB       NOT NULL DEFAULT '[]'
);

-- ── Cost alert configurations ──────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS kc_cost_alert_configs (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    team_id         UUID        NOT NULL REFERENCES kc_teams(id) ON DELETE CASCADE,
    agent_name      TEXT,
    threshold_usd   DOUBLE PRECISION NOT NULL,
    period          TEXT        NOT NULL CHECK (period IN ('hourly','daily','monthly')),
    webhook_url     TEXT,
    notification_email TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Audit log (tamper-evident via HMAC chain) ──────────────────────────────────
CREATE TABLE IF NOT EXISTS kc_audit_log (
    id              BIGSERIAL   PRIMARY KEY,
    team_id         UUID        NOT NULL REFERENCES kc_teams(id) ON DELETE CASCADE,
    user_id         UUID        REFERENCES kc_users(id),
    action          TEXT        NOT NULL,
    resource_type   TEXT        NOT NULL,
    resource_id     TEXT,
    details         JSONB       NOT NULL DEFAULT '{}',
    ip_address      TEXT,
    timestamp       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    chain_hash      TEXT        NOT NULL
);

-- ── Indexes ─────────────────────────────────────────────────────────────────────
CREATE INDEX IF NOT EXISTS kc_runs_team_time_idx    ON kc_runs(team_id, started_at DESC);
CREATE INDEX IF NOT EXISTS kc_runs_agent_name_idx   ON kc_runs(team_id, agent_name, started_at DESC);
CREATE INDEX IF NOT EXISTS kc_runs_status_idx       ON kc_runs(team_id, status);
CREATE INDEX IF NOT EXISTS kc_spans_run_id_idx      ON kc_spans(run_id);
CREATE INDEX IF NOT EXISTS kc_audit_log_team_idx    ON kc_audit_log(team_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS kc_api_keys_team_idx     ON kc_api_keys(team_id);
"#;
