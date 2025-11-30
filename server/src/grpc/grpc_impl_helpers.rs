use std::num::NonZeroU32;
use tonic::Status;
use crate::database::error::DBError;

use lib_clapshot_grpc::proto::org;


pub (crate) fn rpc_expect_field<'a, T> (fld: &'a Option<T>, name: &'a str) -> tonic::Result<&'a T, Status> {
    match fld {
        Some(f) => Ok(f),
        None => return Err(Status::invalid_argument(format!("Missing '{}' field", name))),
    }
}

/// Emulate paging by taking a slice of the vector for database
/// queries that don't support it.
pub (crate) fn paged_vec<T>(v: Vec<T>, p: crate::database::DBPaging) -> Vec<T> {
    v.into_iter().skip(p.offset() as usize).take(p.limit() as usize).collect()
}

/// Convert GRPC paging object to (type-safe) DB counterpart.
/// If it's not present, use an "infinite" page size as a default.
impl TryInto<crate::database::DBPaging> for Option<&org::DbPaging> {
    type Error = Status;

    fn try_into(self) -> tonic::Result<crate::database::DBPaging> {
        match self {
            Some(p) => {
                let page_size = NonZeroU32::new(p.page_size)
                    .ok_or_else(|| Status::invalid_argument("page_size must be > 0"))?;
                Ok(crate::database::DBPaging {
                    page_num: p.page_num.into(),
                    page_size,
                })
            },
            None => Ok(crate::database::DBPaging::default()),
        }
    }
}

/// Convert DBError to Tonic Status
impl From<DBError> for Status {
    fn from(e: DBError) -> Self {
        match e {
            DBError::NotFound() => Status::not_found("DB item not found (on Server)"),
            DBError::BackendError(e) => Status::internal(format!("DB backend error (on Server): {}", e)),
            DBError::Other(e) => Status::internal(format!("DB error (on Server): {}", e)),
        }
    }
}
