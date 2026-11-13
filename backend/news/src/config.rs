use crate::domain::EditorialPolicy;
use sqlx::{
    ConnectOptions,
    postgres::{PgConnectOptions, PgSslMode},
};
use std::{fmt, net::SocketAddr};
use url::Url;

pub struct Config {
    pub bind: SocketAddr,
    pub public_url: Url,
    pub masthead: String,
    pub editorial: EditorialPolicy,
    database: PgConnectOptions,
}

// Explicitly omit database options, credentials and configuration file contents.
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("bind", &self.bind)
            .field("database", &"[redacted]")
            .finish_non_exhaustive()
    }
}

impl Config {
    pub fn editorial_from_env() -> Result<EditorialPolicy, &'static str> {
        Self::editorial_from_lookup(|key| std::env::var(key).ok())
    }

    fn editorial_from_lookup(
        get: impl Fn(&str) -> Option<String>,
    ) -> Result<EditorialPolicy, &'static str> {
        let text = match get("NEWS_EDITORIAL_CONFIG") {
            Some(path) => {
                std::fs::read_to_string(path).map_err(|_| "Cannot read NEWS_EDITORIAL_CONFIG")?
            }
            None => include_str!("../../../config/editorial.json").to_owned(),
        };
        EditorialPolicy::parse(&text)
    }
    pub fn from_env() -> Result<Self, &'static str> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, &'static str> {
        let production = match get("APP_ENV").as_deref().unwrap_or("development") {
            "development" => false,
            "production" => true,
            _ => return Err("APP_ENV must be development or production"),
        };
        let bind = get("BIND_ADDR")
            .unwrap_or_else(|| "127.0.0.1:3001".into())
            .parse()
            .map_err(|_| "BIND_ADDR must be an IP address and port")?;
        let public_url =
            Url::parse(&get("PUBLIC_URL").unwrap_or_else(|| "http://localhost:3001".into()))
                .map_err(|_| "PUBLIC_URL must be an HTTP(S) origin")?;
        if !matches!(public_url.scheme(), "http" | "https")
            || public_url.host_str().is_none()
            || !public_url.username().is_empty()
            || public_url.password().is_some()
            || public_url.query().is_some()
            || public_url.fragment().is_some()
            || public_url.path() != "/"
        {
            return Err("PUBLIC_URL must be an HTTP(S) origin without credentials, path or query");
        }
        if production && public_url.scheme() != "https" {
            return Err("Production PUBLIC_URL must use HTTPS");
        }
        let database_url = get("DATABASE_URL").ok_or("DATABASE_URL is required")?;
        let parsed = Url::parse(&database_url).map_err(|_| "Invalid Postgres DATABASE_URL")?;
        if !matches!(parsed.scheme(), "postgres" | "postgresql")
            || parsed.host_str().is_none()
            || parsed.path().trim_matches('/').is_empty()
        {
            return Err("DATABASE_URL must specify a Postgres host and database");
        }
        let mut database: PgConnectOptions = database_url
            .parse()
            .map_err(|_| "Invalid Postgres connection options")?;
        if production {
            if parsed
                .query_pairs()
                .any(|(key, value)| key == "sslmode" && value != "verify-full")
            {
                return Err("Production database TLS requires sslmode=verify-full");
            }
            database = database.ssl_mode(PgSslMode::VerifyFull);
        }
        let editorial = Self::editorial_from_lookup(&get)?;
        let masthead = get("NEWS_MASTHEAD").unwrap_or_else(|| "The World Brief".into());
        if masthead.trim().is_empty() || masthead.chars().count() > 120 {
            return Err("NEWS_MASTHEAD must contain 1 to 120 characters");
        }
        Ok(Self {
            bind,
            public_url,
            masthead,
            editorial,
            database: database.disable_statement_logging(),
        })
    }

    pub(crate) fn database_options(&self) -> PgConnectOptions {
        self.database.clone().options([
            ("search_path", "news,pg_catalog"),
            ("statement_timeout", "5000"),
            ("lock_timeout", "3000"),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    fn parse(values: &[(&str, &str)]) -> Result<Config, &'static str> {
        let env: HashMap<_, _> = values.iter().copied().collect();
        Config::from_lookup(|key| env.get(key).map(|value| value.to_string()))
    }

    #[test]
    fn explicit_database_and_redacted_debug() {
        assert!(parse(&[]).is_err());
        let config = parse(&[(
            "DATABASE_URL",
            "postgres://reader:fixture-private@localhost/news",
        )])
        .unwrap();
        assert!(!format!("{config:?}").contains("fixture-private"));
        assert!(
            parse(&[(
                "DATABASE_URL",
                "https://reader:fixture-private@localhost/news"
            )])
            .is_err()
        );
    }

    #[test]
    fn production_requires_https_and_verified_database_tls() {
        let base = [
            ("APP_ENV", "production"),
            ("PUBLIC_URL", "https://news.invalid"),
            ("DATABASE_URL", "postgres://reader:fixture@localhost/news"),
        ];
        assert!(parse(&base).is_ok());
        assert!(
            parse(&[
                ("APP_ENV", "production"),
                ("DATABASE_URL", "postgres://localhost/news")
            ])
            .is_err()
        );
        assert!(
            parse(&[
                ("APP_ENV", "production"),
                ("PUBLIC_URL", "https://news.invalid"),
                ("DATABASE_URL", "postgres://localhost/news?sslmode=disable")
            ])
            .is_err()
        );
    }

    #[test]
    fn rejects_ambiguous_public_urls_and_bad_bind_addresses() {
        for url in [
            "https://user:secret@news.invalid",
            "https://news.invalid/path",
            "https://news.invalid/?secret=1",
        ] {
            assert!(
                parse(&[
                    ("DATABASE_URL", "postgres://localhost/news"),
                    ("PUBLIC_URL", url)
                ])
                .is_err()
            );
        }
        assert!(
            parse(&[
                ("DATABASE_URL", "postgres://localhost/news"),
                ("BIND_ADDR", "bad")
            ])
            .is_err()
        );
    }
}
