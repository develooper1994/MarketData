use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("parse error: {0}")]
    ParseError(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("unauthorized")]
    Unauthorized,

    #[error("rate limited")]
    RateLimited,

    #[error("other: {0}")]
    Other(String),
}
