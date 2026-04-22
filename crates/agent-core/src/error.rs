use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("llm error: {0}")]
    Llm(String),

    #[error("tool error: {0}")]
    Tool(String),

    #[error("hook blocked: {0}")]
    Blocked(String),

    #[error("budget exceeded: {0}")]
    BudgetExceeded(String),

    #[error("aborted")]
    Aborted,

    #[error("invalid input: {0}")]
    Invalid(String),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
