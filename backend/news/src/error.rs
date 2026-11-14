use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum AppError {
    NotFound,
    Validation(&'static str),
    Conflict,
    Forbidden,
    Unavailable,
    InvalidData,
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotFound => "Resource not found.",
            Self::Validation(message) => message,
            Self::Conflict => "The operation conflicts with existing or concurrent work.",
            Self::Forbidden => "The database role cannot perform this operation.",
            Self::Unavailable => "The data service is temporarily unavailable.",
            Self::InvalidData => "Stored content failed validation.",
        })
    }
}
impl std::error::Error for AppError {}

impl AppError {
    pub fn database(operation: &'static str, error: sqlx::Error) -> Self {
        if matches!(error, sqlx::Error::RowNotFound) {
            return Self::NotFound;
        }
        if matches!(
            error,
            sqlx::Error::ColumnDecode { .. } | sqlx::Error::Decode(_)
        ) {
            return Self::InvalidData;
        }
        let code = error
            .as_database_error()
            .and_then(|e| e.code())
            .map(|c| c.into_owned());
        let safe_code = code
            .as_deref()
            .filter(|c| c.len() == 5 && c.bytes().all(|b| b.is_ascii_alphanumeric()))
            .unwrap_or("unknown");
        tracing::warn!(operation, code = safe_code, "database operation failed");
        match safe_code {
            "23505" | "23P01" | "40001" | "40P01" | "55P03" => Self::Conflict,
            "42501" => Self::Forbidden,
            "23502" | "23503" | "23514" | "22001" | "22007" | "22008" => {
                Self::Validation("The operation violates a data constraint.")
            }
            _ => Self::Unavailable,
        }
    }
}
