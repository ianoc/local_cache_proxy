use net::buffered_send_stream;
use std::time::Duration;

use hyper::Client;
use std::error::Error as StdError;

use config::AppConfig;
use hyper;
use hyper::body::Payload;
use hyper::service::service_fn;
use std::fmt;
use std::io;
use std::io::ErrorKind as IoErrorKind;

use futures;
use futures::{future, Future};
use http::header;
use http::header::HeaderValue;
use hyper::client::connect::Connect;
use hyper::service::NewService;
use hyper::Server;
use hyper::Uri as HyperUri;
use hyper::{Body, Method, Request, Response, StatusCode};
use net::background_uploader::RequestUpload;
use net::downloader::Downloader;
use net::process_action_cache::{process_action_cache_response, process_existing_action_caches};
use std;
use std::fs;
use std::path::Path;
use std::result::Result;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

fn start_unix_server_impl<S, Bd>(
    _bind_target: &hyper::Uri,
    _s: S,
) -> Result<Box<Future<Item = (), Error = ()> + Send>, ServerError>
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
    unimplemented!()
    // let bind_path = bind_target.authority().unwrap();
    // let svr = hyperlocal::server::Http::new().bind(bind_path, s)?;
    // svr.run()?;
    // Ok(())
}

fn start_http_server_impl<S, Bd>(
    bind_target: &hyper::Uri,
    s: S,
) -> Result<Box<Future<Item = (), Error = ()> + Send>, ServerError>
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

    let server = Server::bind(&socket_addr)
        .serve(s)
        .map_err(|e| eprintln!("server error: {}", e));

    // hyper::rt::run(server);

    Ok(Box::new(server))
}

#[derive(Debug)]
pub enum ServerError {
    IoError(std::io::Error),
    HyperError(::hyper::Error),
    StreamingError(futures::sync::mpsc::SendError<Result<hyper::Chunk, std::io::Error>>),
    StringError(String),
    HttpError(::http::Error),
    InvalidUri(::http::uri::InvalidUri),
    InvalidUriParts(::http::uri::InvalidUriParts),
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.description())
    }
}

impl StdError for ServerError {
    fn description(&self) -> &str {
        match self {
            ServerError::IoError(e) => {
                error!("errr:: {:?}", e);
                StdError::description(e)
            }
            ServerError::HyperError(e) => StdError::description(e),
            ServerError::StreamingError(e) => StdError::description(e),
            ServerError::StringError(e) => e,
            ServerError::HttpError(e) => StdError::description(e),
            ServerError::InvalidUri(e) => StdError::description(e),
            ServerError::InvalidUriParts(e) => StdError::description(e),
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

impl From<String> for ServerError {
    fn from(error: String) -> Self {
        ServerError::StringError(error)
    }
}

impl From<::http::Error> for ServerError {
    fn from(error: ::http::Error) -> Self {
        ServerError::HttpError(error)
    }
}

impl From<::http::uri::InvalidUri> for ServerError {
    fn from(error: ::http::uri::InvalidUri) -> Self {
        ServerError::InvalidUri(error)
    }
}

impl From<::http::uri::InvalidUriParts> for ServerError {
    fn from(error: ::http::uri::InvalidUriParts) -> Self {
        ServerError::InvalidUriParts(error)
    }
}

impl std::convert::From<futures::sync::mpsc::SendError<Result<hyper::Chunk, std::io::Error>>>
    for ServerError
{
    fn from(error: futures::sync::mpsc::SendError<Result<hyper::Chunk, std::io::Error>>) -> Self {
        ServerError::StreamingError(error)
    }
}

type ResponseFuture = Box<Future<Item = Response<Body>, Error = ServerError> + Send>;

fn current_file_size(path: &str) -> Option<u64> {
    match fs::metadata(path) {
        Ok(_meta) => Some(_meta.len()), // file is present
        Err(_e) => None,
    }
}

fn empty_with_status_code(status_code: StatusCode) -> Response<Body> {
    // We decided not to download the file, so 404 it!
    let mut res = Response::new(Body::empty());
    *res.status_mut() = status_code;
    res
}

fn empty_with_status_code_fut(
    status_code: StatusCode,
) -> impl Future<Item = Response<Body>, Error = ServerError> {
    futures::future::ok(empty_with_status_code(status_code))
}

// using from https://github.com/stephank/hyper-staticfile/blob/master/src/static_service.rs

fn send_file(path: String) -> ResponseFuture {
    let metadata = match fs::metadata(&path) {
        Ok(meta) => meta,
        Err(e) => {
            return match e.kind() {
                IoErrorKind::NotFound => panic!(
                    "Should never reach here, file not found looking for {:?}",
                    path
                ),
                IoErrorKind::PermissionDenied => {
                    Box::new(empty_with_status_code_fut(StatusCode::FORBIDDEN).map_err(From::from))
                }
                _ => Box::new(futures::future::err(e).map_err(From::from)),
            };
        }
    };
    // Build response headers.
    let size: String = metadata.len().to_string();
    let mut res = Response::builder();

    let header_size = match HeaderValue::from_str(&size) {
        Err(_e) => {
            return Box::new(
                future::err("Unable to extract size from file system".to_string())
                    .map_err(From::from),
            )
        }
        Ok(s) => s,
    };

    res.header(header::CONTENT_LENGTH, header_size);

    let body = match buffered_send_stream::send_file(&path) {
        Ok(body) => body,
        Err(err) => return Box::new(future::err(err)),
    };
    Box::new(future::result(res.body(body)).map_err(From::from))
}

fn duration_to_float_seconds(d: Duration) -> f64 {
    let f: f64 = d.subsec_nanos() as f64 / 1_000_000_000_0.0;
    f + d.as_secs() as f64
}

fn get_request<C: Connect + 'static>(
    instant: Instant,
    req: Request<Body>,
    downloader: &Downloader,
    http_client: &Client<C>,
    config: &AppConfig,
) -> ResponseFuture {
    info!("Start Get request to {:?}", req.uri());
    let proxy_request = ProxyRequest::new(req.uri());

    let file_name = proxy_request.file_name();

    let data_source_path = Path::new(&config.cache_folder).join(&file_name);
    let downloader = downloader.clone();
    let http_client = http_client.clone();
    let path = req.uri().path().to_string().clone();

    let cfg = config.clone();

    if file_name.starts_with("cas__") {
        let gate_file = current_file_size(&format!("{}/enable_{}", config.cache_folder, file_name));
        let already_present = current_file_size(&format!("{}/{}", config.cache_folder, file_name));
        match already_present.or(gate_file) {
            None => {
                // file we never saw in an action cache message, pretend it doesn't exist.
                info!(
                    "Pretending target doesn't exist {:?}, returning 404",
                    file_name
                );
                let mut res = Response::new(Body::empty());
                *res.status_mut() = StatusCode::NOT_FOUND;
                return Box::new(futures::future::ok(res));
            }
            Some(_) => (),
        }
    }

    Box::new(
        futures::done(proxy_request.build_query_uri(&config.primary_upstream())).and_then(
            move |query_uri| {
                let downloaded_file_future: Box<
                    Future<Item = Option<u64>, Error = ServerError> + Send,
                > = match current_file_size(data_source_path.to_str().unwrap()) {
                    None => Box::new(
                        downloader
                            .fetch_file(&http_client, &query_uri, &file_name)
                            .map_err(From::from),
                    ),
                    Some(len) => Box::new(futures::future::ok(Some(len))),
                };

                let req_uri_string = query_uri.clone();

                let downloaded_fut = downloaded_file_future
                    .and_then(move |file_path| {
                        // info!("Get request issued to : {} --> {:?}", req.uri(), file_path);
                        match file_path {
                            Some(file_len) => {
                                if instant.elapsed().as_secs() >= 2 {
                                    info!(
                                        "[{:?}] took: {} seconds at {} MB/sec",
                                        path,
                                        duration_to_float_seconds(instant.elapsed()),
                                        (file_len as f64 / 1_000_000 as f64)
                                            / duration_to_float_seconds(instant.elapsed())
                                    );
                                }
                                if req.uri().path().starts_with("/ac/") {
                                    process_action_cache_response(&cfg, &file_name)
                                        .map_err(|e| {
                                            warn!(
                                                "Failed to process action cache with: {:?} -- {:?}",
                                                e,
                                                data_source_path.to_str().unwrap().to_string()
                                            );
                                            ()
                                        })
                                        .unwrap_or(());
                                }
                                send_file(data_source_path.to_str().unwrap().to_string())
                            }

                            None => {
                                // info!(
                                //    "[{:?}] took: {} seconds at inf MB/sec",
                                //    path,
                                //    duration_to_float_seconds(instant.elapsed())
                                // );
                                // We decided not to download the file, so 404 it!
                                let mut res = Response::new(Body::empty());
                                *res.status_mut() = StatusCode::NOT_FOUND;
                                Box::new(futures::future::ok(res))
                            }
                        }
                    })
                    .or_else(move |o| {
                        warn!(
                            "Error: {:?} after {} seconds",
                            o,
                            instant.elapsed().as_secs()
                        );
                        let e: ResponseFuture = Box::new(futures::future::ok({
                            let mut res = Response::new(
                                format!("Requested uri: {:?}\n{:?}", req_uri_string, o).into(),
                            );
                            *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                            res
                        }));
                        e
                    });

                downloaded_fut
            },
        ),
    )
}

fn upstream_upload(
    uploader: &RequestUpload,
    upstream_uri: &HyperUri,
    request: &ProxyRequest,
    upload_path: &String,
    cache_folder: String,
    file_name: String,
) {
    let uploader_uri = request.build_query_uri(upstream_uri).unwrap();

    uploader
        .upload(
            &uploader_uri,
            upload_path,
            Box::new(move || {
                if file_name.starts_with("cas__") {
                    match current_file_size(&format!("{}/enable_{}", cache_folder, file_name)) {
                        None => false,
                        Some(_) => true,
                    }
                } else {
                    true
                }
            }),
        )
        .map_err(|e| {
            warn!("Failed to trigger uploader!: {:?}", e);
            ()
        })
        .unwrap();
}

struct ProxyRequest {
    pub repo: String,
    pub tpe: String, // ac or cas
    pub digest: String,
}
impl ProxyRequest {
    pub fn file_name(self: &Self) -> String {
        format!("{}__{}", self.tpe, self.digest).to_string()
    }
    pub fn build_query_uri(self: &Self, upstream_uri: &HyperUri) -> Result<HyperUri, ServerError> {
        let upstream_str = format!("{}", upstream_uri)
            .trim_right_matches('/')
            .to_string();
        format!(
            "{}/{}/repo={}/{}",
            upstream_str, self.tpe, self.repo, self.digest
        ).parse()
            .map_err(From::from)
    }

    pub fn new(uri: &HyperUri) -> ProxyRequest {
        let path: &str = uri.path().trim_matches('/');
        let elements: Vec<&str> = path.split('/').collect();
        if path.starts_with("repo=") {
            let repo_name: &str = {
                let parts: Vec<&str> = elements[0].split('=').collect();
                parts[1]
            };
            ProxyRequest {
                repo: repo_name.to_string(),
                tpe: elements[1].to_string(),
                digest: elements[2].to_string(),
            }
        } else if elements.len() == 2 && (elements[0] == "ac" || elements[0] == "cas") {
            ProxyRequest {
                repo: "unknown".to_string(),
                tpe: elements[0].to_string(),
                digest: elements[1].to_string(),
            }
        } else if path == "/" {
            ProxyRequest {
                repo: "unknown".to_string(),
                tpe: "unknown".to_string(),
                digest: "index.html".to_string(),
            }
        } else {
            ProxyRequest {
                repo: "unknown".to_string(),
                tpe: "unknown".to_string(),
                digest: path.replace('/', "__"),
            }
        }
    }
}

fn put_request(
    instant: Instant,
    req: Request<Body>,
    downloader: &Downloader,
    config: &AppConfig,
    request_upload: &RequestUpload,
) -> ResponseFuture {
    let proxy_request = ProxyRequest::new(req.uri());
    let file_name = proxy_request.file_name();

    info!("Put request: {:?}", req.uri().path());

    let upstream_uri = config.primary_upstream();
    let secondary_upstreams = config.secondary_upstreams();

    let path = req.uri().path().to_string().clone();

    let upload_path = Path::new(&config.cache_folder)
        .join(&file_name)
        .to_str()
        .unwrap()
        .to_string();

    let uploader = request_upload.clone();
    let cache_folder = config.cache_folder.clone();

    let processor_config = config.clone();
    Box::new(
        downloader
            .save_file(&file_name, req)
            .map(move |_file| {
                match _file {
                    Some(_f) => {
                        process_action_cache_response(&processor_config, &_f)
                            .map_err(|e| {
                                warn!("Failed to process action cache with: {:?} for {:?}", e, _f);
                                ()
                            })
                            .unwrap_or(());

                        upstream_upload(
                            &uploader,
                            &upstream_uri,
                            &proxy_request,
                            &upload_path,
                            cache_folder.clone(),
                            file_name.clone(),
                        );
                        for u in secondary_upstreams.iter() {
                            upstream_upload(
                                &uploader,
                                &u,
                                &proxy_request,
                                &upload_path,
                                cache_folder.clone(),
                                file_name.clone(),
                            );
                        }
                        ()
                    }
                    None => (),
                };

                if instant.elapsed().as_secs() > 60 {
                    info!(
                        "Put request to {:?} took {} seconds",
                        path,
                        instant.elapsed().as_secs()
                    );
                }
                empty_with_status_code(StatusCode::CREATED)
            })
            .map_err(|e| {
                warn!("Error doing put! {:?}", e);
                From::from(e)
            }),
    )
}

pub struct State(pub Instant);

pub fn start_server<C: Connect + 'static>(
    config: &AppConfig,
    downloader: Downloader,
    http_client: Client<C>,
) -> Result<(), io::Error>
where
    C: Connect,
{
    let s = Arc::new(Mutex::new(State(Instant::now())));

    process_existing_action_caches(config.clone());

    let (uploader, channel) = ::net::background_uploader::start_uploader(config, &http_client, &s);
    let terminator = ::net::terminator::start_terminator(config, &s);

    let cfg = config.clone();

    let new_service = move || {
        // Move a clone of `client` into the `service_fn`.
        let downloader = downloader.clone();
        let http_client = http_client.clone();
        let request_upload = channel.clone();
        let state = Arc::clone(&s);

        let inner_cfg = cfg.clone();
        service_fn(move |req| {
            {
                let mut locked = state.lock().unwrap();
                locked.0 = Instant::now();
            }
            Box::new(
                match req.method() {
                    &Method::GET => get_request(
                        Instant::now(),
                        req,
                        &downloader,
                        &http_client,
                        &inner_cfg.clone(),
                    ),
                    &Method::PUT => put_request(
                        Instant::now(),
                        req,
                        &downloader,
                        &inner_cfg.clone(),
                        &request_upload,
                    ),
                    _ => {
                        info!(
                            "Attempted {:?} operation to {:?}",
                            req.method(),
                            req.uri().path()
                        );
                        Box::new(empty_with_status_code_fut(
                            StatusCode::INTERNAL_SERVER_ERROR,
                        ))
                    }
                }.map_err(|e| {
                    error!("Ran into error: {}", e.description());
                    e
                }),
            )
        })
    };

    let server_engine = match config.bind_target.scheme() {
        Some("unix") => {
            info!(
                "Going to remove existing socket path: {}",
                config.bind_target.authority().unwrap()
            );

            fs::remove_file(config.bind_target.authority().unwrap())?;
            info!(
                "Going to bind/start server on unix socket for: {}",
                config.bind_target
            );
            start_unix_server_impl(&config.bind_target, new_service).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Error configuring unix server: {:?}", e),
                )
            })?
        }
        Some("http") => {
            info!(
                "Going to bind/start server on http path for: 127.0.0.1:{}",
                config.bind_target.port().unwrap()
            );
            start_http_server_impl(&config.bind_target, new_service).map_err(|e| {
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
                    config.bind_target
                ),
            ))
        }
    };

    hyper::rt::run(
        server_engine
            .join(uploader)
            .join(terminator)
            .map(|_| ())
            .map_err(|_| ()),
    );
    return Ok(());
}
