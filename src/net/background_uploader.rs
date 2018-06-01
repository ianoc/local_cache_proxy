use std::sync::Mutex;
use std::sync::Arc;
use futures::Poll;
use config::AppConfig;
use hyper::client::connect::Connect;
use hyper::Client;
use http::Uri;
use net::buffered_send_stream;
use http::Request;
use hyper::StatusCode;
use futures::Future;
use futures;
use futures::future::Either;
use futures::stream::Stream;
use tokio::prelude::*;
use futures::sync::mpsc;

use std::time::Duration;


#[derive(Debug)]
struct UploadRequest {
    uri: Uri,
    path: String,
}

type Tx = mpsc::UnboundedSender<UploadRequest>;
type Rx = mpsc::UnboundedReceiver<UploadRequest>;

#[derive(Debug)]
pub(super) struct RequestUpload(Arc<Mutex<Tx>>);

impl Clone for RequestUpload {
    fn clone(self: &Self) -> Self {
        RequestUpload(Arc::clone(&self.0))
    }
}

impl RequestUpload {
    pub(super) fn upload(self: Self, uri: &Uri, path: &String) -> Result<(), String> {
        let uploader = self.0.lock().map_err(|e| e.to_string())?;
        uploader
            .unbounded_send(UploadRequest {
                uri: uri.clone(),
                path: path.clone(),
            })
            .map_err(|e| e.to_string())
    }
}

pub(super) fn start_uploader<C: Connect + 'static>(
    _config: &AppConfig,
    http_client: &Client<C>,
) -> (Box<Future<Item = (), Error = ()> + Send>, RequestUpload) {
    // Create a channel for this peer
    let (tx, rx) = mpsc::unbounded();

    (
        Box::new(Uploader {
            client: http_client.clone(),
            active_future: None,
            rx: rx,
        }),
        RequestUpload(Arc::new(Mutex::new(tx))),
    )
}

struct Uploader<C> {
    /// Name of the peer. This is the first line received from the client.
    client: Client<C>,

    /// this is the active worker future that might be complete
    active_future: Option<Box<Future<Item = (), Error = String> + Send + 'static>>,

    /// Receive half of the message channel.
    ///
    /// This is used to receive messages from peers. When a message is received
    /// off of this `Rx`, it will be written to the socket.
    rx: Rx,
}

impl<C> Future for Uploader<C>
where
    C: Connect + 'static,
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        // Receive all messages from peers.

        match &mut self.active_future {
            Some(fut) => {
                match fut.poll() {
                    Ok(Async::Ready(_)) => (),
                    Err(e) => {
                        warn!("Failed to poll active future with {:?}", e);
                        ()
                    }
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                }
            }
            None => (),
        };
        self.active_future = None;


        let upload_request: UploadRequest = match try_ready!(self.rx.poll().map_err(|e| {
            warn!("Failed to poll rx queue with {:?}", e);
            ()
        })) {
            Some(u) => u,
            None => {
                error!("Uploader Terminating");
                return Ok(Async::Ready(()));
            }
        };


        self.active_future = Some(run_upload_file(
            self.client.clone(),
            upload_request.uri,
            upload_request.path,
        ));

        futures::task::current().notify();
        Ok(Async::NotReady)
    }
}



pub(super) fn raw_upload_file<C: Connect + 'static>(
    http_client: Client<C>,
    uri: Uri,
    path: String,
) -> Box<Future<Item = (), Error = String> + Send + 'static> {
    info!("Uploading {} to {:?}", path, uri);
    let body = match buffered_send_stream::send_file(&path) {
        Ok(body) => body,
        Err(e) => {
            return Box::new(futures::future::err(e).map_err(
                |e: ::std::io::Error| e.to_string(),
            ))
        }
    };


    let http_payload = http_client.request(Request::put(uri.clone()).body(body).unwrap());


    Box::new(http_payload.map_err(|e| e.to_string()).map(|_| ()))
}


fn run_upload_file<C: Connect + 'static>(
    http_client: Client<C>,
    uri: Uri,
    path: String,
) -> Box<Future<Item = (), Error = String> + Send + 'static> {
    info!("Maybe uploading {} to {:?}", path, uri);
    let resp_uri = uri.clone();
    let ee_resp_uri = uri.clone();
    Box::new(
        ::net::client::connect_for_head(
            http_client.clone(),
            uri.clone(),
            10,
            Duration::from_millis(500),
            4,
        ).map_err(|e| {
            warn!("Error in check if file exists: {:?}", e);
        })
            .and_then(move |resp| if let Some(true) = resp.map(|_e| true) {
                info!(
                    "Content already present for {:?}, skipping upload -- {:?}",
                    resp_uri,
                    resp
                );
                Either::A(futures::future::ok(()))
            } else {
                Either::B(
                    raw_upload_file(http_client, uri, path)
                        .map_err(|e| {
                            warn!("Error in upload: {:?}", e);
                        })
                        .map(|_e| ()),
                )
            })
            .map(|_| ())
            .map_err(move |_e| {
                warn!("Connecting error for {:?}", ee_resp_uri);
                "None".to_string()
            }),
    )
}
