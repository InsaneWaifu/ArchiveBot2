// @generated automatically by Diesel CLI.

diesel::table! {
    objects (id) {
        id -> Integer,
        path -> Text,
        name -> Text,
        size -> BigInt,
        expiry_unix -> BigInt,
        user -> BigInt,
    }
}

diesel::table! {
    sharex_config (user_id) {
        user_id -> BigInt,
        json -> Text,
    }
}

diesel::table! {
    users (snowflake) {
        snowflake -> BigInt,
        name_cached -> Nullable<Text>,
    }
}

diesel::joinable!(sharex_config -> users (user_id));

diesel::allow_tables_to_appear_in_same_query!(
    objects,
    sharex_config,
    users,
);
