//! Local accounts, opaque server-side sessions and explicit editorial permissions.
use crate::{error::AppError, persistence};
use argon2::{
    Argon2, Params,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use axum::http::{HeaderMap, header};
use cookie::{Cookie, SameSite};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use tokio::sync::Semaphore;
use zeroize::Zeroizing;

// A valid-cost dummy hash, not an account or a known password. Unknown/locked
// identities still execute Argon2. The all-zero output is not an authentication bypass.
const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c3ludGhldGljLXNhbHQ$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Editor,
    Publisher,
}
impl Role {
    pub fn parse(value: &str) -> Result<Self, AppError> {
        match value {
            "editor" => Ok(Self::Editor),
            "publisher" => Ok(Self::Publisher),
            _ => Err(AppError::Validation("Role must be editor or publisher.")),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Editor => "editor",
            Self::Publisher => "publisher",
        }
    }
}
#[derive(Clone, Copy)]
pub enum Permission {
    Edit,
    Publish,
}
#[derive(Clone)]
pub struct Principal {
    id: String,
    role: Role,
}
impl Principal {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn role(&self) -> Role {
        self.role
    }
    pub fn require(&self, permission: Permission) -> Result<(), AuthError> {
        if matches!(permission, Permission::Publish) && self.role != Role::Publisher {
            Err(AuthError::Forbidden)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone)]
pub struct Session {
    principal: Principal,
    token_hash: String,
    csrf: Zeroizing<String>,
}
impl Session {
    pub fn principal(&self) -> &Principal {
        &self.principal
    }
    pub(crate) fn csrf(&self) -> &str {
        &self.csrf
    }
}

pub(crate) async fn begin_authorised(
    pool: &PgPool,
    session: &Session,
    permission: Permission,
) -> Result<(Transaction<'static, Postgres>, Principal), AppError> {
    if !persistence::schema_available(pool).await {
        return Err(AppError::Unavailable);
    }
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::database("editorial_begin", e))?;
    match live_principal(&mut tx, session, permission).await {
        Ok(principal) => Ok((tx, principal)),
        Err(error) => {
            let _ = tx.rollback().await;
            if error == AppError::Forbidden {
                record_service_denial(pool, session).await?;
            }
            Err(error)
        }
    }
}

/// Lock identity before session, matching operator revocation's lock order. The
/// caller retains these locks through its editorial commit, never across UI work.
pub(crate) async fn live_principal(
    tx: &mut Transaction<'_, Postgres>,
    session: &Session,
    permission: Permission,
) -> Result<Principal, AppError> {
    let role = sqlx::query_scalar::<_, String>(
        "SELECT role FROM news.admin_users WHERE id=$1 AND active FOR SHARE",
    )
    .bind(session.principal.id())
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| AppError::database("editorial_identity", e))?
    .ok_or(AppError::Forbidden)?;
    let valid = sqlx::query_scalar::<_,String>("SELECT user_id FROM news.admin_sessions WHERE token_hash=$1 AND user_id=$2 AND revoked_at IS NULL AND expires_at>clock_timestamp() FOR SHARE")
        .bind(&session.token_hash).bind(session.principal.id()).fetch_optional(&mut **tx).await.map_err(|e|AppError::database("editorial_session",e))?;
    if valid.is_none() {
        return Err(AppError::Forbidden);
    }
    let principal = Principal {
        id: session.principal.id().into(),
        role: Role::parse(&role).map_err(|_| AppError::InvalidData)?,
    };
    principal
        .require(permission)
        .map_err(|_| AppError::Forbidden)?;
    Ok(principal)
}

pub(crate) async fn record_service_denial(
    pool: &PgPool,
    session: &Session,
) -> Result<(), AppError> {
    // A stale bearer is not attributed as a freshly authenticated actor.
    sqlx::query(
        "INSERT INTO news.admin_audit(subject_id,action) VALUES ($1,'authorization_denied')",
    )
    .bind(session.principal.id())
    .execute(pool)
    .await
    .map_err(|e| AppError::database("editorial_denial", e))?;
    Ok(())
}
#[derive(Debug, PartialEq, Eq)]
pub enum AuthError {
    Unauthenticated,
    Forbidden,
    Throttled,
    InvalidInput,
    Unavailable,
}

struct LoginWindow {
    start: Instant,
    attempts: u32,
}
pub struct AuthService {
    pool: PgPool,
    origin: String,
    secure: bool,
    password_slots: Arc<Semaphore>,
    login_window: Mutex<LoginWindow>,
}
pub struct Login {
    session: Zeroizing<String>,
    csrf: Zeroizing<String>,
}
impl AuthService {
    pub fn new(pool: PgPool, public_url: &url::Url) -> Self {
        Self {
            pool,
            origin: public_url.origin().ascii_serialization(),
            secure: public_url.scheme() == "https",
            password_slots: Arc::new(Semaphore::new(2)),
            login_window: Mutex::new(LoginWindow {
                start: Instant::now(),
                attempts: 0,
            }),
        }
    }
    pub fn verify_origin(&self, headers: &HeaderMap) -> Result<(), AuthError> {
        let mut origins = headers.get_all(header::ORIGIN).iter();
        if origins.next().and_then(|o| o.to_str().ok()) != Some(self.origin.as_str())
            || origins.next().is_some()
        {
            return Err(AuthError::Forbidden);
        }
        Ok(())
    }
    fn admit_login(&self) -> Result<(), AuthError> {
        let mut window = self
            .login_window
            .lock()
            .map_err(|_| AuthError::Unavailable)?;
        if window.start.elapsed() >= Duration::from_secs(60) {
            window.start = Instant::now();
            window.attempts = 0;
        }
        if window.attempts >= 30 {
            return Err(AuthError::Throttled);
        }
        window.attempts += 1;
        Ok(())
    }
    pub async fn login(
        &self,
        username: &str,
        password: Zeroizing<String>,
    ) -> Result<Login, AuthError> {
        self.admit_login()?;
        if username.len() > 64 || password.len() > 1024 {
            return Err(AuthError::InvalidInput);
        }
        let permit = self
            .password_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| AuthError::Throttled)?;
        if !persistence::schema_available(&self.pool).await {
            return Err(AuthError::Unavailable);
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        #[derive(sqlx::FromRow)]
        struct User {
            id: String,
            password_hash: String,
            active: bool,
            locked: bool,
            attempts: i16,
        }
        let user = sqlx::query_as::<_,User>("SELECT id,password_hash,active,COALESCE(locked_until>now(),false) AS locked,(CASE WHEN locked_until<=now() THEN 0 ELSE failed_attempts END)::smallint AS attempts FROM news.admin_users WHERE id=$1 FOR UPDATE")
            .bind(username).fetch_optional(&mut *tx).await.map_err(db)?;
        let usable = user.as_ref().is_some_and(|u| u.active && !u.locked);
        let hash = if usable {
            user.as_ref().expect("usable user").password_hash.clone()
        } else {
            DUMMY_HASH.to_owned()
        };
        let verified = tokio::task::spawn_blocking(move || {
            let _permit = permit; // Remains held even if the HTTP future is cancelled.
            verify_password(&hash, &password)
        })
        .await
        .map_err(|_| AuthError::Unavailable)??;
        if !usable || !verified {
            if let Some(user) = &user
                && user.active
                && !user.locked
            {
                let attempts = user.attempts + 1;
                sqlx::query("UPDATE news.admin_users SET failed_attempts=$2,locked_until=CASE WHEN $2>=5 THEN now()+interval '10 minutes' ELSE NULL END WHERE id=$1")
                        .bind(&user.id).bind(attempts).execute(&mut *tx).await.map_err(db)?;
            }
            audit(
                &mut tx,
                None,
                user.as_ref().map(|u| u.id.as_str()),
                "login_failed",
            )
            .await?;
            tx.commit().await.map_err(db)?;
            return Err(AuthError::Unauthenticated);
        }
        let user = user.expect("verified existing user");
        let login = Login {
            session: random_token()?,
            csrf: random_token()?,
        };
        sqlx::query("UPDATE news.admin_users SET failed_attempts=0,locked_until=NULL WHERE id=$1")
            .bind(&user.id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query("INSERT INTO news.admin_sessions(token_hash,csrf_hash,user_id,expires_at) VALUES ($1,$2,$3,now()+interval '8 hours')")
            .bind(digest(&login.session)).bind(digest(&login.csrf)).bind(&user.id).execute(&mut *tx).await.map_err(db)?;
        audit(&mut tx, Some(&user.id), Some(&user.id), "login_succeeded").await?;
        tx.commit().await.map_err(db)?;
        Ok(login)
    }
    pub async fn authenticate(&self, headers: &HeaderMap) -> Result<Session, AuthError> {
        let token =
            read_cookie(headers, self.cookie_names()[0])?.ok_or(AuthError::Unauthenticated)?;
        let csrf =
            read_cookie(headers, self.cookie_names()[1])?.ok_or(AuthError::Unauthenticated)?;
        if !valid_token(&token) || !valid_token(&csrf) {
            return Err(AuthError::Unauthenticated);
        }
        if !persistence::schema_available(&self.pool).await {
            return Err(AuthError::Unavailable);
        }
        let token_hash = digest(&token);
        let row = sqlx::query_as::<_,(String,String,String)>("SELECT u.id,u.role,s.csrf_hash FROM news.admin_sessions s JOIN news.admin_users u ON u.id=s.user_id WHERE s.token_hash=$1 AND s.revoked_at IS NULL AND s.expires_at>now() AND u.active")
            .bind(&token_hash).fetch_optional(&self.pool).await.map_err(db)?.ok_or(AuthError::Unauthenticated)?;
        if !bool::from(digest(&csrf).as_bytes().ct_eq(row.2.as_bytes())) {
            return Err(AuthError::Unauthenticated);
        }
        let role = Role::parse(&row.1).map_err(|_| AuthError::Unavailable)?;
        Ok(Session {
            principal: Principal { id: row.0, role },
            token_hash,
            csrf,
        })
    }
    pub async fn authorise(
        &self,
        session: &Session,
        permission: Permission,
    ) -> Result<(), AuthError> {
        if let Err(error) = session.principal.require(permission) {
            self.record_denial(session).await?;
            return Err(error);
        }
        Ok(())
    }
    pub async fn record_denial(&self, session: &Session) -> Result<(), AuthError> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        audit(
            &mut tx,
            Some(session.principal.id()),
            Some(session.principal.id()),
            "authorization_denied",
        )
        .await?;
        tx.commit().await.map_err(db)?;
        Ok(())
    }
    pub fn verify_csrf(
        &self,
        headers: &HeaderMap,
        session: &Session,
        supplied: &str,
    ) -> Result<(), AuthError> {
        self.verify_origin(headers)?;
        if !valid_token(supplied) || !bool::from(supplied.as_bytes().ct_eq(session.csrf.as_bytes()))
        {
            return Err(AuthError::Forbidden);
        }
        Ok(())
    }
    pub async fn logout(&self, session: &Session) -> Result<(), AuthError> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("UPDATE news.admin_sessions SET revoked_at=now() WHERE token_hash=$1 AND revoked_at IS NULL")
            .bind(&session.token_hash).execute(&mut *tx).await.map_err(db)?;
        audit(
            &mut tx,
            Some(session.principal.id()),
            Some(session.principal.id()),
            "logout",
        )
        .await?;
        tx.commit().await.map_err(db)?;
        Ok(())
    }
    pub(crate) fn cookies(&self, login: Option<&Login>) -> [String; 2] {
        let values = login
            .map(|l| [l.session.as_str(), l.csrf.as_str()])
            .unwrap_or(["", ""]);
        let names = self.cookie_names();
        std::array::from_fn(|index| {
            Cookie::build((names[index], values[index]))
                .path("/")
                .http_only(true)
                .secure(self.secure)
                .same_site(SameSite::Strict)
                .max_age(if login.is_some() {
                    cookie::time::Duration::hours(8)
                } else {
                    cookie::time::Duration::ZERO
                })
                .build()
                .to_string()
        })
    }
    fn cookie_names(&self) -> [&'static str; 2] {
        if self.secure {
            ["__Host-worldnews_session", "__Host-worldnews_csrf"]
        } else {
            ["worldnews_session", "worldnews_csrf"]
        }
    }
}

fn db(error: sqlx::Error) -> AuthError {
    let _ = AppError::database("admin_identity", error);
    AuthError::Unavailable
}
async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    actor: Option<&str>,
    subject: Option<&str>,
    action: &'static str,
) -> Result<(), AuthError> {
    sqlx::query("INSERT INTO news.admin_audit(actor_id,subject_id,action) VALUES ($1,$2,$3)")
        .bind(actor)
        .bind(subject)
        .bind(action)
        .execute(&mut **tx)
        .await
        .map_err(db)?;
    Ok(())
}
fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn random_token() -> Result<Zeroizing<String>, AuthError> {
    let mut bytes = Zeroizing::new([0u8; 32]);
    getrandom::fill(bytes.as_mut()).map_err(|_| AuthError::Unavailable)?;
    Ok(Zeroizing::new(
        bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
    ))
}
fn valid_token(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn read_cookie(headers: &HeaderMap, name: &str) -> Result<Option<Zeroizing<String>>, AuthError> {
    let mut result = None;
    for header in headers.get_all(header::COOKIE) {
        let text = header.to_str().map_err(|_| AuthError::Unauthenticated)?;
        for cookie in Cookie::split_parse(text) {
            let cookie = cookie.map_err(|_| AuthError::Unauthenticated)?;
            if cookie.name() == name {
                if result.is_some() {
                    return Err(AuthError::Unauthenticated);
                }
                result = Some(Zeroizing::new(cookie.value().to_owned()));
            }
        }
    }
    Ok(result)
}
fn verify_password(hash: &str, password: &str) -> Result<bool, AuthError> {
    let parsed = PasswordHash::new(hash).map_err(|_| AuthError::Unavailable)?;
    let params = Params::try_from(&parsed).map_err(|_| AuthError::Unavailable)?;
    if parsed.algorithm.as_str() != "argon2id"
        || parsed.version != Some(19)
        || params.m_cost() != Params::DEFAULT_M_COST
        || params.t_cost() != Params::DEFAULT_T_COST
        || params.p_cost() != Params::DEFAULT_P_COST
    {
        return Err(AuthError::Unavailable);
    }
    match Argon2::default().verify_password(password.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(argon2::password_hash::Error::PasswordInvalid) => Ok(false),
        Err(_) => Err(AuthError::Unavailable),
    }
}
pub fn valid_username(value: &str) -> bool {
    (3..=64).contains(&value.len())
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b))
}
pub async fn bootstrap(
    pool: &PgPool,
    username: &str,
    role: Role,
    password: Zeroizing<String>,
) -> Result<(), AppError> {
    if !valid_username(username)
        || password.chars().count() < 16
        || password.len() > 1024
        || password.chars().any(char::is_control)
    {
        return Err(AppError::Validation(
            "Username or password does not meet bootstrap requirements.",
        ));
    }
    if !persistence::schema_available(pool).await {
        return Err(AppError::Unavailable);
    }
    let hash = tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|h| h.to_string())
            .map_err(|_| AppError::Unavailable)
    })
    .await
    .map_err(|_| AppError::Unavailable)??;
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::database("admin_bootstrap", e))?;
    sqlx::query("INSERT INTO news.admin_users(id,password_hash,role) VALUES ($1,$2,$3)")
        .bind(username)
        .bind(hash)
        .bind(role.as_str())
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::database("admin_bootstrap", e))?;
    audit(&mut tx, None, Some(username), "account_created")
        .await
        .map_err(|_| AppError::Unavailable)?;
    tx.commit()
        .await
        .map_err(|e| AppError::database("admin_bootstrap", e))?;
    Ok(())
}
pub async fn disable(pool: &PgPool, username: &str) -> Result<(), AppError> {
    if !valid_username(username) {
        return Err(AppError::Validation("Invalid username."));
    }
    if !persistence::schema_available(pool).await {
        return Err(AppError::Unavailable);
    }
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::database("admin_disable", e))?;
    let result = sqlx::query("UPDATE news.admin_users SET active=false WHERE id=$1")
        .bind(username)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::database("admin_disable", e))?;
    if result.rows_affected() != 1 {
        return Err(AppError::NotFound);
    }
    sqlx::query(
        "UPDATE news.admin_sessions SET revoked_at=now() WHERE user_id=$1 AND revoked_at IS NULL",
    )
    .bind(username)
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::database("admin_disable", e))?;
    audit(&mut tx, None, Some(username), "account_disabled")
        .await
        .map_err(|_| AppError::Unavailable)?;
    tx.commit()
        .await
        .map_err(|e| AppError::database("admin_disable", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn tokens_cookies_and_origin_validation_are_strict() {
        let config = crate::config::Config::from_lookup(|key| {
            (key == "DATABASE_URL").then(|| "postgres://localhost/news".into())
        })
        .unwrap();
        let service = AuthService::new(
            persistence::pool(&config),
            &url::Url::parse("https://news.invalid").unwrap(),
        );
        let mut headers = HeaderMap::new();
        for origin in [
            "https://news.invalid.evil.invalid",
            "http://news.invalid",
            "null",
        ] {
            headers.insert(header::ORIGIN, origin.parse().unwrap());
            assert_eq!(service.verify_origin(&headers), Err(AuthError::Forbidden));
        }
        headers.insert(header::ORIGIN, "https://news.invalid".parse().unwrap());
        assert!(service.verify_origin(&headers).is_ok());
        headers.append(header::ORIGIN, "https://news.invalid".parse().unwrap());
        assert!(service.verify_origin(&headers).is_err());
        headers.insert(
            header::COOKIE,
            "worldnews_session=a; worldnews_session=b".parse().unwrap(),
        );
        assert!(read_cookie(&headers, "worldnews_session").is_err());
        assert!(!valid_username("admin@example.invalid"));
        assert!(!valid_username("Admin"));
        assert!(valid_username("test-editor"));
        assert!(!valid_token(&"g".repeat(64)));
        assert!(valid_token(&random_token().unwrap()));
    }
    #[test]
    fn dummy_hash_is_valid_cost_and_never_authenticates_the_synthetic_password() {
        assert_eq!(
            verify_password(DUMMY_HASH, "synthetic-incorrect-password"),
            Ok(false)
        );
        assert!(
            verify_password(&DUMMY_HASH.replace("m=19456", "m=999999999"), "anything").is_err()
        );
        let editor = Principal {
            id: "synthetic-editor".into(),
            role: Role::Editor,
        };
        assert!(editor.require(Permission::Edit).is_ok());
        assert_eq!(
            editor.require(Permission::Publish),
            Err(AuthError::Forbidden)
        );
    }
}
