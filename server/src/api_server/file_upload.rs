use futures_util::stream::StreamExt;
use warp::http::HeaderMap;
use futures::stream::TryStreamExt;
use mpart_async::server::MultipartStream;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::video_pipeline::IncomingFile;
use super::parse_auth_headers;
use super::server_state::ServerState;
use super::user_session::{org_authz_with_default, AuthzTopic, AuthzError};

use lib_clapshot_grpc::proto;
use proto::org::authz_user_action_request as authz_req;


/// Warp filter for multipart/form-data file upload
///
/// # Arguments
/// * `upload_dir` - Path to the directory where the uploaded files will be stored
/// * `upload_done` - Channel to submit the uploaded file path to further processing
/// * `mime` - Parsed mime options from the request
/// * `hdrs` - Authentication headers to be used for identifying the uploader
/// * `server` - Server state (for organizer connection)
/// * `body` - The request body (stream)
pub async fn handle_multipart_upload(
    upload_dir: std::path::PathBuf,
    upload_done: crossbeam_channel::Sender<IncomingFile>,
    mime: mime::Mime,
    hdrs: HeaderMap,
    server: ServerState,
    body: impl warp::Stream<Item = Result<impl bytes::Buf, warp::Error>> + Unpin)
        -> Result<warp::reply::WithStatus<String>, Infallible>
{
    let (user_id, user_name, cookies) = parse_auth_headers(&hdrs);

    // Check from organizer if user is allowed to upload.
    // Allow by default if organizer is not configured or doesn't care.
    if let Some(uri) = &server.organizer_uri {
        if server.organizer_has_connected.load(std::sync::atomic::Ordering::Relaxed) {
            let organizer = match crate::grpc::grpc_client::connect(uri.clone()).await {
                Ok(c) => Arc::new(tokio::sync::Mutex::new(c)),
                Err(e) => {
                    tracing::error!("Failed to connect to organizer: {}", e);
                    return Ok(warp::reply::with_status("Internal error: failed to connect to organizer".into(), warp::http::StatusCode::INTERNAL_SERVER_ERROR));
                }
            };

            let org_session = proto::org::UserSessionData {
                sid: "<upload--not-set>".to_string(),
                user: Some(proto::UserInfo { id: user_id.clone(), name: Some(user_name.clone()) }),
                cookies
            };

            match org_authz_with_default(&org_session, "upload video", true, &server, &Some(organizer),
                true, AuthzTopic::Other(None, authz_req::other_op::Op::UploadVideo)).await {
                Ok(_) => {},
                Err(AuthzError::Denied) => {
                    return Ok(warp::reply::with_status("Permission denied".into(), warp::http::StatusCode::FORBIDDEN));
                },
            }
        }
    }

    // Parse the multipart stream
    let boundary = mime.get_param("boundary").map(|v| v.to_string());
    let boundary = match boundary {
        Some(b) => b,
        None => return Ok(warp::reply::with_status("Missing boundary".into(), warp::http::StatusCode::BAD_REQUEST)),
    };
    let mut stream = MultipartStream::new(boundary, body.map_ok(|mut buf| buf.copy_to_bytes(buf.remaining())));
    let mut uploaded_file: PathBuf = PathBuf::new();

    while let Ok(Some(mut field)) = stream.try_next().await {
        match field.name().unwrap_or("unknown".into()).as_ref() {
            "fileupload" => {
                match field.filename().map(String::from) {
                    Err(e) => {
                        let msg = format!("Error getting filename: {}", e);
                        tracing::error!(msg);
                        return Ok(warp::reply::with_status(msg, warp::http::StatusCode::BAD_REQUEST));
                    },
                    Ok(filename) =>
                    {
                        let path = Path::new(&filename);
                        if path.file_name() != Some(path.as_os_str()) {
                            return Ok(warp::reply::with_status("Filename must not contain path".into(), warp::http::StatusCode::BAD_REQUEST));
                        }

                        // Make a unique upload dir
                        let uuid = uuid::Uuid::new_v4();
                        let new_dir = async_std::path::PathBuf::from(&upload_dir).join(uuid.to_string());
                        let dst =  new_dir.join(path.file_name().unwrap());
                        if dst.exists().await {
                            tracing::error!("Upload dst '{}' already exists, even tough it was prefixed with uuid4. Bug??", dst.display());
                            return Ok(warp::reply::with_status("Internal error: file already exists".into(), warp::http::StatusCode::INTERNAL_SERVER_ERROR));
                        }
                        if let Err(e) = async_std::fs::create_dir_all(&new_dir).await {
                            tracing::error!("Failed to create upload dir: {}", e);
                            return Ok(warp::reply::with_status("Internal error: failed to create upload dir".into(), warp::http::StatusCode::INTERNAL_SERVER_ERROR));
                        }

                        // Create the file and stream the data into it
                        match async_std::fs::File::create(&dst).await {
                            Err(e) => {
                                let msg = format!("Failed to create file '{}': {}", dst.display(), e);
                                tracing::error!(msg);
                                return Ok(warp::reply::with_status(msg, warp::http::StatusCode::INTERNAL_SERVER_ERROR));
                            },
                            Ok(mut f) =>
                            {
                                // Read and write in parallel
                                let (buff_tx, mut buff_rx) = tokio::sync::mpsc::channel::<bytes::Bytes>(16);

                                // Read chunks from HTTP
                                let read_all_chunks = async move {
                                    while let Some(chunk) = field.next().await {
                                        match chunk {
                                            Ok(data) => { buff_tx.send(data).await.unwrap(); },
                                            Err(e) => { return Err(e.to_string()); }
                                    }}; Ok(())  // buff_tx dropped
                                };

                                // Write chunks to the file
                                let write_all_chunks = async move {
                                    while let Some(data) = buff_rx.recv().await {
                                        futures_util::AsyncWriteExt::write_all(&mut f, &data).await
                                            .map_err(|e| e.to_string())?;
                                    }; Ok(())
                                };

                                // Run both tasks in parallel, cleanup on error
                                if let Err(e) = tokio::try_join!(read_all_chunks, write_all_chunks)
                                {
                                    tracing::error!("Upload failed: {}", e);
                                    // Remove the file & dir, since it's incomplete
                                    if let Err(e) = async_std::fs::remove_file(&dst).await {
                                        tracing::warn!("Failed to remove incomplete upload file: {}", e);
                                    } else if let Err(e) = async_std::fs::remove_dir(new_dir).await {
                                        tracing::warn!("Failed to remove incomplete upload dir: {}", e);
                                    }
                                    return Ok(warp::reply::with_status(format!("Upload failed: {e}"), warp::http::StatusCode::BAD_REQUEST));
                                }
                                tracing::info!("File uploaded: '{:?}'", dst);
                                uploaded_file = dst.into();
                            }
                        };
                    }
                }
            },
            fieldname => {
                tracing::info!("Skipping UNKNOWN multipart POST field '{fieldname}'");
            },
        }
    }

    if let Err(e) = upload_done.send(IncomingFile{ file_path: uploaded_file, user_id: user_id }) {
        tracing::error!("Failed to send upload ok signal: {:?}", e);
        return Ok(warp::reply::with_status("Internal error: failed to send upload ok signal".into(), warp::http::StatusCode::INTERNAL_SERVER_ERROR));
    }
    Ok(warp::reply::with_status("Ok".into(), warp::http::StatusCode::OK))
}
