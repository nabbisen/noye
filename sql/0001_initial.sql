-- =============================================================
-- Noye: 死活監視システム 初期スキーマ
-- =============================================================

-- ユーザーテーブル (RBAC: 管理者 / 会員)
CREATE TABLE IF NOT EXISTS users (
    id          TEXT PRIMARY KEY,
    email       TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    role        TEXT NOT NULL CHECK (role IN ('admin', 'member')),
    is_active   INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_users_email ON users(email);

-- 監視対象テーブル (要件2-2)
CREATE TABLE IF NOT EXISTS targets (
    id               TEXT PRIMARY KEY,
    name             TEXT NOT NULL,
    type             TEXT NOT NULL CHECK (type IN ('http', 'https', 'tcp', 'smtp', 'tls')),
    host             TEXT NOT NULL,
    port             INTEGER,
    path             TEXT DEFAULT '/',
    expected_status  INTEGER DEFAULT 200,
    body_contains    TEXT,                        -- レスポンス本文検証文字列
    tls_threshold_days INTEGER DEFAULT 30,       -- TLS有効期限しきい値(日)
    timeout_sec      INTEGER NOT NULL DEFAULT 10,
    retry_count      INTEGER NOT NULL DEFAULT 3,
    interval_minutes INTEGER NOT NULL DEFAULT 5,
    is_disabled      INTEGER NOT NULL DEFAULT 0,
    owner_id         TEXT NOT NULL,
    tags             TEXT,                        -- JSON配列形式のタグ
    next_check_at    TEXT NOT NULL DEFAULT (datetime('now')),
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
    created_by       TEXT NOT NULL,
    updated_by       TEXT NOT NULL,
    FOREIGN KEY(owner_id) REFERENCES users(id)
);
CREATE INDEX idx_targets_next_check ON targets(next_check_at) WHERE is_disabled = 0;
CREATE INDEX idx_targets_owner ON targets(owner_id);
CREATE INDEX idx_targets_type ON targets(type);

-- 状態管理テーブル (要件2-4: 連続失敗/成功のトラッキング)
CREATE TABLE IF NOT EXISTS target_states (
    target_id               TEXT PRIMARY KEY,
    current_status          TEXT NOT NULL DEFAULT 'unknown'
                            CHECK (current_status IN ('up', 'down', 'degraded', 'maintenance', 'unknown')),
    consecutive_successes   INTEGER NOT NULL DEFAULT 0,
    consecutive_failures    INTEGER NOT NULL DEFAULT 0,
    success_threshold       INTEGER NOT NULL DEFAULT 3,  -- 復旧判定しきい値
    failure_threshold       INTEGER NOT NULL DEFAULT 3,  -- 障害判定しきい値
    last_checked_at         TEXT,
    last_status_change_at   TEXT,
    last_notification_at    TEXT,                         -- 重複通知防止
    FOREIGN KEY(target_id) REFERENCES targets(id) ON DELETE CASCADE
);

-- 監視結果テーブル (時系列データ)
CREATE TABLE IF NOT EXISTS check_results (
    id              TEXT PRIMARY KEY,
    target_id       TEXT NOT NULL,
    checked_at      TEXT NOT NULL DEFAULT (datetime('now')),
    is_success      INTEGER NOT NULL,
    status_code     INTEGER,
    response_time_ms INTEGER,
    error_message   TEXT,
    tls_expiry_date TEXT,                        -- TLS証明書の有効期限
    tls_days_left   INTEGER,
    details         TEXT,                         -- JSON形式の詳細情報
    FOREIGN KEY(target_id) REFERENCES targets(id) ON DELETE CASCADE
);
CREATE INDEX idx_results_target_time ON check_results(target_id, checked_at DESC);
CREATE INDEX idx_results_checked_at ON check_results(checked_at);

-- 障害イベントテーブル (状態遷移の記録)
CREATE TABLE IF NOT EXISTS incidents (
    id              TEXT PRIMARY KEY,
    target_id       TEXT NOT NULL,
    status          TEXT NOT NULL CHECK (status IN ('open', 'resolved', 'acknowledged')),
    opened_at       TEXT NOT NULL DEFAULT (datetime('now')),
    resolved_at     TEXT,
    duration_sec    INTEGER,
    cause           TEXT,
    resolution_note TEXT,
    created_by      TEXT,
    FOREIGN KEY(target_id) REFERENCES targets(id) ON DELETE CASCADE
);
CREATE INDEX idx_incidents_target ON incidents(target_id, opened_at DESC);
CREATE INDEX idx_incidents_status ON incidents(status);

-- メンテナンス期間テーブル (要件2-4: 通知抑止)
CREATE TABLE IF NOT EXISTS maintenance_windows (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    start_at        TEXT NOT NULL,
    end_at          TEXT NOT NULL,
    target_tag      TEXT,                         -- タグ単位指定 (NULLの場合は全対象)
    target_id       TEXT,                         -- 個別対象指定
    suppress_notify INTEGER NOT NULL DEFAULT 1,
    is_active       INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    created_by      TEXT NOT NULL,
    updated_by      TEXT NOT NULL,
    FOREIGN KEY(target_id) REFERENCES targets(id) ON DELETE SET NULL
);
CREATE INDEX idx_maint_active ON maintenance_windows(start_at, end_at) WHERE is_active = 1;

-- 通知チャネル設定テーブル
CREATE TABLE IF NOT EXISTS notification_channels (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    channel_type    TEXT NOT NULL CHECK (channel_type IN ('webhook', 'email', 'slack')),
    endpoint        TEXT NOT NULL,                -- URL or email address
    is_enabled      INTEGER NOT NULL DEFAULT 1,
    owner_id        TEXT NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY(owner_id) REFERENCES users(id)
);

-- 対象と通知チャネルの関連テーブル
CREATE TABLE IF NOT EXISTS target_notifications (
    target_id   TEXT NOT NULL,
    channel_id  TEXT NOT NULL,
    on_down     INTEGER NOT NULL DEFAULT 1,
    on_up       INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY(target_id, channel_id),
    FOREIGN KEY(target_id) REFERENCES targets(id) ON DELETE CASCADE,
    FOREIGN KEY(channel_id) REFERENCES notification_channels(id) ON DELETE CASCADE
);

-- 監査ログテーブル (要件4-1)
CREATE TABLE IF NOT EXISTS audit_logs (
    id              TEXT PRIMARY KEY,
    action_time     TEXT NOT NULL DEFAULT (datetime('now')),
    actor_id        TEXT NOT NULL,
    actor_email     TEXT,
    resource_type   TEXT NOT NULL,                -- target, user, maintenance, notification, etc.
    resource_id     TEXT,
    action_type     TEXT NOT NULL,                -- create, update, delete, login, manual_resolve, etc.
    previous_value  TEXT,                          -- JSON形式の変更前値
    new_value       TEXT,                          -- JSON形式の変更後値
    result          TEXT NOT NULL DEFAULT 'success' CHECK (result IN ('success', 'failure')),
    ip_address      TEXT,
    FOREIGN KEY(actor_id) REFERENCES users(id)
);
CREATE INDEX idx_audit_time ON audit_logs(action_time DESC);
CREATE INDEX idx_audit_actor ON audit_logs(actor_id);
CREATE INDEX idx_audit_resource ON audit_logs(resource_type, resource_id);

-- データライフサイクル管理用メタテーブル
CREATE TABLE IF NOT EXISTS retention_policies (
    table_name      TEXT PRIMARY KEY,
    retention_days  INTEGER NOT NULL,
    archive_to_r2   INTEGER NOT NULL DEFAULT 0,
    last_cleanup_at  TEXT
);

-- デフォルトの保持期間を設定
INSERT OR IGNORE INTO retention_policies (table_name, retention_days, archive_to_r2) VALUES
    ('check_results', 90, 1),
    ('incidents', 365, 1),
    ('audit_logs', 365, 1);
