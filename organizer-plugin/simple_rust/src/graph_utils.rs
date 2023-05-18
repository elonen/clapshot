use lib_clapshot_grpc::{proto::{self, org}};
use tonic::Status;

use crate::{GrpcServerConn, folder_ops::UserNodeData, RpcResult};



/// Name of the client cookie that holds the current folder path
pub const PATH_COOKIE_NAME: &str = "folder_path";

/// Node type for the singleton node that represents a unique user id.
/// Singleton key holds the user id. Body is a json-encoded UserNodeData.
pub const USER_ID_NODE_TYPE: &str = "user_id";

/// Node type for folders
pub const FOLDER_NODE_TYPE: &str = "folder";

/// Edge from video/folder to folder that contains it
pub const PARENT_FOLDER_EDGE_TYPE: &str = "parent_folder";

/// Edge from folder to user id
pub const OWNER_EDGE_TYPE: &str = "owner";



/// Make sure the given user id is formally valid.
pub fn validate_user_id_syntax(id: &str) -> Result<(), Status> {
    if id.is_empty() {
        return Err(Status::invalid_argument("User id cannot be empty"));
    }
    if id.starts_with("|") {
        return Err(Status::invalid_argument("User id cannot start with '|' (reserved for internal use)"));
    }
    Ok(())
}


/// Get or create a singleton PropNode of the given type.
/// 
/// Call this inside a transaction (does multiple dependent DB calls)
async fn mkget_singleton_node(srv: &mut GrpcServerConn, node_type: &str, singleton_key: &str, body: Option<String>) -> RpcResult<org::PropNode>
{
    let get_res = srv.db_get_singleton_prop_node(org::DbGetSingletonPropNodeRequest {
        node_type: node_type.into(),
        singleton_key: singleton_key.into(),
    }).await?.into_inner();

    if let Some(node) = get_res.node {
        return Ok(node);
    }

    let ins_res = srv.db_upsert(org::DbUpsertRequest {
            nodes: vec![org::PropNode {
                id: "".into(),  // empty = insert
                body,
                node_type: node_type.into(),
                singleton_key: Some(singleton_key.into()),
            }],
            ..Default::default()
        }).await?.into_inner();

    return Ok(ins_res.nodes.first()
        .ok_or(Status::internal("BUG: No node returned from insert"))?.clone())
}


/// Get or create a PropNode for a user ID.
///
/// Call this inside a transaction (does multiple dependent DB calls)
pub (crate) async fn mkget_session_user(srv: &mut GrpcServerConn, user: &proto::UserInfo)
    -> RpcResult<org::PropNode>
{
    validate_user_id_syntax(&user.id)?;
    let user_node = mkget_singleton_node(
        srv,
        USER_ID_NODE_TYPE,
        &user.id,
        Some( serde_json::to_string(&UserNodeData { name: user.name.clone()}).unwrap())
    ).await?;
    Ok(user_node)
}
