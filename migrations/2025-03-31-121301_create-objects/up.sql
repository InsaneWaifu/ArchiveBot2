-- Your SQL goes here
CREATE TABLE objects (
    id INTEGER PRIMARY KEY NOT NULL,
    path TEXT NOT NULL,
    name TEXT NOT NULL,
    size BigInt NOT NULL,
    expiry_unix BigInt NOT NULL,
    user BigInt NOT NULL
);