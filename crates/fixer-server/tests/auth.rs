use std::str::FromStr;

use fixer_server::{
    SqliteJobStore,
    auth::password::{hash_password, verify_password},
};
use sqlx::{Row, SqlitePool, sqlite::SqliteConnectOptions};

#[test]
fn passwords_use_argon2id_phc_values_and_never_debug_as_plaintext() {
    let password = "correct horse battery staple";
    let encoded = hash_password(password).unwrap();

    assert!(encoded.as_str().starts_with("$argon2id$v=19$"));
    assert!(verify_password(password, &encoded).unwrap());
    assert!(!verify_password("wrong password", &encoded).unwrap());
    assert!(!format!("{encoded:?}").contains(password));
    assert!(!format!("{encoded:?}").contains(encoded.as_str()));
    assert!(hash_password("").is_err());
}

#[tokio::test]
async fn api_tokens_are_shown_once_but_only_sha256_digests_are_persisted() {
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("auth.sqlite3");
    let store = SqliteJobStore::open(&database).await.unwrap();

    let issued = store.issue_api_token("automation").await.unwrap();
    let presented = issued.token().to_owned();
    assert!(presented.starts_with("fixer_pat_"));
    assert!(presented.len() >= "fixer_pat_".len() + 43);
    assert!(!format!("{issued:?}").contains(&presented));
    assert_eq!(
        store.authenticate_api_token(&presented).await.unwrap(),
        Some(issued.id())
    );
    assert_eq!(
        store
            .authenticate_api_token("fixer_pat_not-a-real-secret")
            .await
            .unwrap(),
        None
    );

    let pool = SqlitePool::connect_with(
        SqliteConnectOptions::from_str(database.to_str().unwrap())
            .unwrap()
            .foreign_keys(true),
    )
    .await
    .unwrap();
    let row = sqlx::query("SELECT name, token_digest FROM api_tokens WHERE id = ?")
        .bind(issued.id())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>("name"), "automation");
    let digest = row.get::<Vec<u8>, _>("token_digest");
    assert_eq!(digest.len(), 32);
    assert_ne!(digest, presented.as_bytes());
    let schema = sqlx::query("SELECT sql FROM sqlite_schema WHERE name = 'api_tokens'")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get::<String, _>("sql");
    assert!(!schema.contains("token TEXT"));

    assert!(store.revoke_api_token(issued.id()).await.unwrap());
    assert_eq!(
        store.authenticate_api_token(&presented).await.unwrap(),
        None
    );
    pool.close().await;
}

#[tokio::test]
async fn password_login_issues_expiring_digest_only_sessions_with_csrf_secrets() {
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("sessions.sqlite3");
    let store = SqliteJobStore::open(&database).await.unwrap();
    let encoded = hash_password("session password").unwrap();
    store.set_password_hash(&encoded).await.unwrap();

    assert!(
        store
            .verify_single_user_password("session password")
            .await
            .unwrap()
    );
    assert!(
        !store
            .verify_single_user_password("wrong password")
            .await
            .unwrap()
    );
    let issued = store
        .create_session(std::time::Duration::from_secs(60))
        .await
        .unwrap();
    assert!(issued.token().starts_with("fixer_session_"));
    assert!(issued.csrf_token().starts_with("fixer_csrf_"));
    assert!(!format!("{issued:?}").contains(issued.token()));
    assert!(
        store
            .authenticate_session(issued.token(), Some(issued.csrf_token()))
            .await
            .unwrap()
    );
    assert!(
        !store
            .authenticate_session(issued.token(), Some("wrong csrf"))
            .await
            .unwrap()
    );
    assert!(store.revoke_session(issued.token()).await.unwrap());
    assert!(
        !store
            .authenticate_session(issued.token(), None)
            .await
            .unwrap()
    );

    let pool = SqlitePool::connect_with(
        SqliteConnectOptions::from_str(database.to_str().unwrap())
            .unwrap()
            .foreign_keys(true),
    )
    .await
    .unwrap();
    let password: String = sqlx::query_scalar("SELECT password_hash FROM single_user_auth")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(password, encoded.as_str());
    let session_columns = sqlx::query("PRAGMA table_info(auth_sessions)")
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect::<Vec<_>>();
    assert_eq!(
        session_columns,
        [
            "token_digest",
            "csrf_digest",
            "created_at_ms",
            "expires_at_ms"
        ]
    );
    pool.close().await;
}
