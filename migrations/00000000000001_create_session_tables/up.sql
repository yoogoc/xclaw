CREATE TABLE sessions (
    id            TEXT PRIMARY KEY NOT NULL,
    binding_id    TEXT NOT NULL UNIQUE,
    active_thread_id TEXT,
    auto_approved_tools TEXT NOT NULL DEFAULT '[]',
    metadata      TEXT NOT NULL DEFAULT 'null',
    created_at    TEXT NOT NULL,
    last_active_at TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_sessions_binding ON sessions(binding_id);

CREATE TABLE threads (
    id                 TEXT PRIMARY KEY NOT NULL,
    session_id         TEXT NOT NULL,
    user_id            TEXT NOT NULL,
    channel            TEXT NOT NULL,
    external_thread_id TEXT,
    state              TEXT NOT NULL DEFAULT 'Idle',
    metadata           TEXT NOT NULL DEFAULT 'null',
    pending_approvals  TEXT NOT NULL DEFAULT '[]',
    created_at         TEXT NOT NULL,
    updated_at         TEXT NOT NULL
);

CREATE INDEX idx_threads_session ON threads(session_id);
CREATE INDEX idx_threads_routing ON threads(session_id, user_id, channel, external_thread_id);

CREATE TABLE turns (
    id                      TEXT PRIMARY KEY NOT NULL,
    thread_id               TEXT NOT NULL,
    session_id              TEXT NOT NULL,
    turn_number             INTEGER NOT NULL,
    user_input              TEXT NOT NULL,
    thinking                TEXT,
    response                TEXT,
    state                   TEXT NOT NULL DEFAULT 'Processing',
    started_at              TEXT NOT NULL,
    completed_at            TEXT,
    error                   TEXT,
    current_tool_iterations INTEGER NOT NULL DEFAULT 0,
    draft_message_id        TEXT
);

CREATE INDEX idx_turns_thread ON turns(thread_id);
CREATE INDEX idx_turns_order  ON turns(thread_id, turn_number);

CREATE TABLE turn_tool_calls (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    turn_id     TEXT NOT NULL,
    call_index  INTEGER NOT NULL,
    name        TEXT NOT NULL,
    parameters  TEXT NOT NULL DEFAULT '{}',
    result      TEXT,
    error       TEXT
);

CREATE INDEX idx_toolcalls_turn ON turn_tool_calls(turn_id);
CREATE UNIQUE INDEX idx_toolcalls_order ON turn_tool_calls(turn_id, call_index);