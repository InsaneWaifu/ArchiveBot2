-- Your SQL goes here
CREATE TABLE sharex_config (
    user_id BigInt NOT NULL PRIMARY KEY REFERENCES users(snowflake),
    json TEXT NOT NULL
)
