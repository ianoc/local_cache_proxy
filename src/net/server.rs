use hyper::Client;
use hyper::service::service_fn;
use hyper::body::Payload;
// use hyper::service::{service_fn};
use hyper;
use futures::Stream;
use config::AppConfig;
use hyperlocal;
use std::io;
use hyper::Error as HyperError;
use std::result::Result;
use net::Downloader;
use hyper::client::connect::Connect;
use futures;
// use hyper::server::{Http, Response, const_service};
use hyper::service::NewService;
use std::fs;
// use hyper::server::Service;
use hyper::Server;
use std;
use hyper::Uri as HyperUri;
use http::uri::Parts;
use futures::{future, Future};
use hyper::{Body, Method, Request, Response, StatusCode};

use std::path::Path;


const PHRASE: &'static str = "It's a Unix system. I know this.";


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

type ResponseFuture = Box<Future<Item = Response<Body>, Error = io::Error> + Send>;


fn response_examples<C: Connect + 'static>(
    req: Request<Body>,
    downloader: &Downloader,
    http_client: &Client<C>,
    config: &AppConfig,
) -> ResponseFuture {


    let mut parts = Parts::from(config.upstream());
    parts.path_and_query = match req.uri().path_and_query() {
        Some(path_and_query_ref) => Some(path_and_query_ref.clone()),
        None => None,
    };
    let query_uri = HyperUri::from_parts(parts).unwrap();

    let downloaded = downloader.fetch_file(http_client, &query_uri);
    let req_uri_string = query_uri.clone();
    let cache_folder = config.cache_folder.clone();
    Box::new(
        downloaded
            .and_then(move |file_path| {

                match file_path {
                    Some(file_name) => {
                        let data_source_path = Path::new(&cache_folder).join(&file_name);
                        info!("data_source_path: {:?}", data_source_path);

                        futures::future::ok(match (req.method(), req.uri().path()) {
                            (&Method::GET, "/") => {
                                Response::new(PHRASE.into())
                                // .with_header(ContentLength(PHRASE.len() as u64))
                            }
                            _ => {
                                let mut res = Response::new(Body::empty());
                                *res.status_mut() = StatusCode::NOT_FOUND;
                                res
                            }
                        })

                    }
                    None => {
                        // We decided not to download the file, so 404 it!
                        let mut res = Response::new(Body::empty());
                        *res.status_mut() = StatusCode::NOT_FOUND;
                        futures::future::ok(res)
                    }
                }
            })
            .or_else(move |o| {
                warn!("Error: {:?}", o);
                futures::future::ok({
                    let mut res = Response::new(
                        format!("Requested uri: {:?}\n{:?}", req_uri_string, o).into(),
                    );
                    *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                    res
                })
            }),
    )
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
