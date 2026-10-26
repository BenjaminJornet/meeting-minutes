CREATE TABLE summary_profiles (
    endpoint TEXT NOT NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    options_json TEXT NOT NULL,
    PRIMARY KEY (endpoint, provider, model)
);
CREATE TABLE summary_checkpoints (
    meeting_id TEXT NOT NULL,
    run_key TEXT NOT NULL,
    step_key TEXT NOT NULL,
    input_hash TEXT NOT NULL,
    output TEXT NOT NULL,
    PRIMARY KEY (run_key, step_key, input_hash)
);
CREATE INDEX summary_checkpoints_meeting ON summary_checkpoints(meeting_id);
CREATE TABLE summary_progress (
    meeting_id TEXT PRIMARY KEY,
    stage TEXT NOT NULL DEFAULT 'pending',
    completed INTEGER NOT NULL DEFAULT 0,
    total INTEGER NOT NULL DEFAULT 0,
    error_code TEXT,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
