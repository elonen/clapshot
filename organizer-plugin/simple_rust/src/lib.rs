use tonic::{Request, Response, Status};

pub mod proto {
    tonic::include_proto!("clapshot.organizer");
    pub const FILE_DESCRIPTOR_SET: &[u8] = tonic::include_file_descriptor_set!("organizer_descriptor");
}

#[derive(Debug, Default)]
pub struct SimpleOrganizer {}
type RpcResult<T> = Result<Response<T>, Status>;


// Implement the RCP methods

#[tonic::async_trait]
impl proto::organizer_server::Organizer for SimpleOrganizer
{
    async fn server_started(&self, req: Request<proto::ServerInfo>) -> RpcResult<proto::Empty>
    {
        tracing::info!("Got a request: {:?}", req);
        Ok(Response::new(proto::Empty {}))
    }

    async fn define_actions(&self, req: Request<proto::NamedActions>) -> RpcResult<proto::Empty>
    {
        tracing::info!("Got a request: {:?}", req);
        Ok(Response::new(proto::Empty {}))
    }
}
