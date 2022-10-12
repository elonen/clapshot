use tokio::task::JoinError;

#[derive(Debug)]
pub enum DBError {
    NotFound(),
    BackendError(diesel::result::Error),
    Other(String),
}

pub type DBResult<T> = Result<T, DBError>;
pub type EmptyDBResult = Result<(), DBError>;


// Conversions

impl From<diesel::result::Error> for DBError {
    fn from(e: diesel::result::Error) -> Self {
        DBError::BackendError(e)
    }
}

impl From<JoinError> for DBError {
    fn from(e: JoinError) -> Self {
        DBError::Other(e.to_string())
    }
}

impl From<DBError> for Box<dyn std::error::Error> {
    fn from(e: DBError) -> Self {
        match e {
            DBError::NotFound() => "DB row now found".into(),
            DBError::BackendError(e) => Box::new(e),
            DBError::Other(txt) => txt.into(),
        }
    }
}

impl std::fmt::Display for DBError {

    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DBError::NotFound() => write!(f, "Not found"),
            DBError::BackendError(e) => write!(f, "Backend error: {}", e),
            DBError::Other(s) => write!(f, "Other error: {}", s),
        }
    }
}
