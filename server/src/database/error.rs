use thiserror;
use anyhow;

#[derive(thiserror::Error, Debug)]
pub enum DBError {
    #[error("DB entry not found")]
    NotFound(),
    #[error("DB backend error: {0}")]
    BackendError(#[from] diesel::result::Error),
    #[error("Other DB error: {0}")]
    Other(#[from] anyhow::Error),
}

pub type DBResult<T> = Result<T, DBError>;
pub type EmptyDBResult = Result<(), DBError>;
