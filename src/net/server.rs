use futures::sync::mpsc::SendError;
use hyper::Chunk;
use hyper::Client;
use std::error::Error as StdError;

use hyper::service::service_fn;
use hyper::body::Payload;
use hyper;
use futures::Stream;
use config::AppConfig;
use hyperlocal;
use std::io;
use std::io::{Read, ErrorKind as IoErrorKind};
use std::io::Error;
use std::mem;
use std::fmt;

use std::result::Result;
use net::Downloader;
use hyper::client::connect::Connect;
use futures;
use hyper::service::NewService;
use hyper::Server;
use std;
use hyper::Uri as HyperUri;
use http::uri::{Parts, PathAndQuery};
use std::fs::{self, File};
use bytes::Bytes;
use futures::{future, Future, Async, Poll};
use hyper::{Body, Method, Request, Response, StatusCode};
use std::path::Path;


fn start_unix_server_impl<S, Bd>(bind_target: &hyper::Uri, s: S) -> Result<(), ServerError>
where
    S: Sync,
    S: NewService<ReqBody = Body, ResBody = Bd> + Send + 'static,
    S::Error: Into<Box<::std::error::Error + Send + Sync>>,
    S::Service: Send,
    S::Error: Into<Box<::std::error::Error + Send + Sync>>,
    <S as ::hyper::service::NewService>::Future: Send,
    <S::Service as ::hyper::service::Service>::Future: Send + 'static,
    Bd: Payload,
{
    let bind_path = bind_target.authority().unwrap();
    let svr = hyperlocal::server::Http::new().bind(bind_path, s)?;
    svr.run()?;
    Ok(())
}


fn start_http_server_impl<S, Bd>(bind_target: &hyper::Uri, s: S) -> Result<(), ServerError>
where
    S: NewService<ReqBody = Body, ResBody = Bd> + Send + 'static,
    S::Error: Into<Box<::std::error::Error + Send + Sync>>,
    S::Service: Send,
    S::Error: Into<Box<::std::error::Error + Send + Sync>>,
    <S as ::hyper::service::NewService>::Future: Send,
    <S::Service as ::hyper::service::Service>::Future: Send + 'static,
    Bd: Payload,
{

    let socket_addr = format!("127.0.0.1:{}", bind_target.port().unwrap())
        .parse()
        .unwrap();


    let server = Server::bind(&socket_addr).serve(s).map_err(|e| {
        eprintln!("server error: {}", e)
    });

    hyper::rt::run(server);

    Ok(())
}


#[derive(Debug)]
pub enum ServerError {
    IoError(std::io::Error),
    HyperError(::hyper::Error),
    BindError(::hyperlocal::server::BindError),
    StreamingError(futures::sync::mpsc::SendError<Result<hyper::Chunk, std::io::Error>>),
    StringError(String),
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.description())
    }
}

impl StdError for ServerError {
    fn description(&self) -> &str {
        match self {
            ServerError::IoError(e) => StdError::description(e),
            ServerError::HyperError(e) => StdError::description(e),
            ServerError::BindError(_e) => "Bind error",
            ServerError::StreamingError(e) => StdError::description(e),
            ServerError::StringError(e) => e,
        }
    }
}
impl From<std::io::Error> for ServerError {
    fn from(error: std::io::Error) -> Self {
        ServerError::IoError(error)
    }
}

impl From<::hyper::Error> for ServerError {
    fn from(error: ::hyper::Error) -> Self {
        ServerError::HyperError(error)
    }
}

impl From<::hyperlocal::server::BindError> for ServerError {
    fn from(error: ::hyperlocal::server::BindError) -> Self {
        ServerError::BindError(error)
    }
}
impl From<String> for ServerError {
    fn from(error: String) -> Self {
        ServerError::StringError(error)
    }
}

impl std::convert::From<futures::sync::mpsc::SendError<Result<hyper::Chunk, std::io::Error>>>
    for ServerError {
    fn from(error: futures::sync::mpsc::SendError<Result<hyper::Chunk, std::io::Error>>) -> Self {
        ServerError::StreamingError(error)
    }
}


type ResponseFuture = Box<Future<Item = Response<Body>, Error = ServerError> + Send>;



fn should_try_fetch(path: &str) -> bool {
    match fs::metadata(path) {
        Ok(_meta) => false, // file is present
        Err(e) => {
            match e.kind() {
                IoErrorKind::NotFound => true,
                _ => false,
            }
        }

    }
}


pub fn build_file_name(uri: &HyperUri) -> String {
    match uri.path() {
        "/" => "index.html".to_string(),
        o => o.trim_matches('/').replace('/', "__"),
    }
}

fn empty_with_status_code(
    status_code: StatusCode,
) -> impl Future<Item = Response<Body>, Error = io::Error> {
    // We decided not to download the file, so 404 it!
    let mut res = Response::new(Body::empty());
    *res.status_mut() = status_code;
    futures::future::ok(res)
}

// using from https://github.com/stephank/hyper-staticfile/blob/master/src/static_service.rs
struct FileChunkStream(File);
impl Stream for FileChunkStream {
    type Item = Result<Chunk, Error>;
    type Error = SendError<Self::Item>;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        // TODO: non-blocking read
        let mut buf: [u8; 16384] = unsafe { mem::uninitialized() };
        match self.0.read(&mut buf) {
            Ok(0) => Ok(Async::Ready(None)),
            Ok(size) => {
                futures::task::current().notify();
                Ok(Async::Ready(Some(Ok(Chunk::from(buf[0..size].to_owned())))))
            }
            Err(err) => Ok(Async::Ready(Some(Err(err)))),
        }
    }
}

struct BufferedSendStream {
    file_chunk_stream: FileChunkStream,
    sender: hyper::body::Sender,
}
impl BufferedSendStream {
    fn new(file_chunk_stream: FileChunkStream, sender: hyper::body::Sender) -> BufferedSendStream {
        BufferedSendStream {
            file_chunk_stream: file_chunk_stream,
            sender: sender,
        }
    }
}
impl Future for BufferedSendStream {
    type Item = ();
    type Error = ServerError;

    fn poll(&mut self) -> Result<Async<()>, ServerError> {
        loop {
            self.sender.poll_ready()?;

            match self.file_chunk_stream.poll()? {
                Async::Ready(None) => return Ok(Async::Ready(())),
                Async::Ready(Some(Ok(buf))) => {
                    self.sender.send_data(buf).map_err(|_e| {
                        "Failed to send chunk".to_string()
                    })?;
                    return Ok(Async::NotReady);
                }
                Async::Ready(Some(Err(e))) => {
                    warn!("Failed to send file, error: {:?}", e);
                    return Ok(Async::Ready(()));
                }
                Async::NotReady => return Ok(Async::NotReady),
            }

        }
    }
}

fn send_file(path: String) -> ResponseFuture {
    let metadata = match fs::metadata(&path) {
        Ok(meta) => meta,
        Err(e) => {
            return match e.kind() {
                IoErrorKind::NotFound => {
                    panic!(
                        "Should never reach here, file not found looking for {:?}",
                        path
                    )
                }
                IoErrorKind::PermissionDenied => Box::new(
                    empty_with_status_code(StatusCode::FORBIDDEN)
                        .map_err(From::from),
                ),
                _ => Box::new(futures::future::err(e).map_err(From::from)),
            };
        }
    };
    // Build response headers.
    let size = metadata.len();
    let mut res = Response::builder();
    // .header(header::CONTENT_LENGTH, &size.to_string());


    let file = match File::open(path) {
        Ok(file) => file,
        Err(err) => return Box::new(future::err(err).map_err(From::from)),
    };

    let (sender, body) = Body::channel();
    hyper::rt::spawn(
        BufferedSendStream::new(FileChunkStream(file), sender)
            .map(|_| ())
            .map_err(|_| ()),
    );

    Box::new(future::ok(res.body(body).unwrap()))
}


fn response_examples<C: Connect + 'static>(
    req: Request<Body>,
    downloader: &Downloader,
    http_client: &Client<C>,
    config: &AppConfig,
) -> ResponseFuture {

    let file_name = build_file_name(req.uri());
    let data_source_path = Path::new(&config.cache_folder).join(&file_name);


    let mut parts = Parts::from(config.upstream());
    parts.path_and_query =
        match (
            config.upstream().path_and_query(),
            req.uri().path_and_query(),
        ) {
            (Some(upstream_path_and_query), Some(path_and_query_ref)) => Some(
                PathAndQuery::from_shared(Bytes::from(
                    upstream_path_and_query.path().to_owned() +
                        path_and_query_ref.path(),
                )).unwrap(),
            ),
            (None, Some(path_and_query_ref)) => Some(path_and_query_ref.clone()),
            (_, None) => None,
        };
    let query_uri = HyperUri::from_parts(parts).unwrap();


    let downloaded_file_future: Box<Future<Item = Option<String>, Error = ServerError> + Send> =
        if should_try_fetch(data_source_path.to_str().unwrap()) {
            Box::new(
                downloader
                    .fetch_file(http_client, &query_uri, &file_name)
                    .map_err(From::from),
            )
        } else {
            Box::new(futures::future::ok(Some(file_name)))
        };

    let req_uri_string = query_uri.clone();

    let downloaded_fut = downloaded_file_future
        .and_then(move |file_path| {

            match file_path {
                Some(_file_name) => send_file(data_source_path.to_str().unwrap().to_string()),

                None => {
                    // We decided not to download the file, so 404 it!
                    let mut res = Response::new(Body::empty());
                    *res.status_mut() = StatusCode::NOT_FOUND;
                    Box::new(futures::future::ok(res))
                }
            }
        })
        .or_else(move |o| {
            warn!("Error: {:?}", o);
            let e: ResponseFuture = Box::new(futures::future::ok({
                let mut res = Response::new(
                    format!("Requested uri: {:?}\n{:?}", req_uri_string, o).into(),
                );
                *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                res
            }));
            e
        });

    Box::new(downloaded_fut)
}


pub fn start_server<C: Connect + 'static>(
    _config: AppConfig,
    downloader: Downloader,
    http_client: Client<C>,
) -> Result<(), io::Error>
where
    C: Connect,
{

    let cfg = _config.clone();
    let new_service = move || {
        // Move a clone of `client` into the `service_fn`.
        let downloader = downloader.clone();
        let http_client = http_client.clone();
        let config = cfg.clone();

        service_fn(move |req| {
            response_examples(req, &downloader, &http_client, &config)
        })
    };


    match _config.bind_target.scheme() {
        Some("unix") => {
            debug!(
                "Going to remove existing socket path: {}",
                _config.bind_target.authority().unwrap()
            );

            fs::remove_file(_config.bind_target.authority().unwrap())?;
            info!(
                "Going to bind/start server on unix socket for: {}",
                _config.bind_target
            );
            start_unix_server_impl(&_config.bind_target, new_service)
                .map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("Error configuring unix server: {:?}", e),
                    )
                })?
        }
        Some("http") => {
            info!(
                "Going to bind/start server on http path for: 127.0.0.1:{}",
                _config.bind_target.port().unwrap()
            );
            start_http_server_impl(&_config.bind_target, new_service)
                .map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("Error configuring unix server: {:?}", e),
                    )
                })?
        }

        _o => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "Invalid bind target {}, didn't understand the scheme",
                    _config.bind_target
                ),
            ))
        }
    }
    return Ok(());
}
