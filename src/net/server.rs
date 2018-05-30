use hyper::server::service_fn;
use hyper::header::{ContentType, ContentLength};
use hyper::server::NewService;
use hyper;
use futures::Stream;
use config::AppConfig;
use hyperlocal;
use std::io;
use hyper::Error as HyperError;
use std::result::Result;
use net::Downloader;
use hyper::client::Connect;
use futures;
use hyper::{Body, Method, Request, Server, StatusCode};
use hyper::server::{Http, Response, const_service};
use std::fs;
use hyper::server::Service;
use futures::future::FutureResult;
use hyper::server;
use std;
use futures::Future;
use tokio;

const PHRASE: &'static str = "It's a Unix system. I know this.";


fn start_unix_server_impl<S, B>(bind_target: &hyper::Uri, s: S) -> Result<(), HyperError>
where
    S: NewService<Request = Request, Response = Response<B>, Error = ::hyper::Error>
        + Send
        + Sync
        + 'static,
    B: Stream<Error = ::hyper::Error> + 'static,
    B::Item: AsRef<[u8]>,
{
    let bind_path = bind_target.authority().unwrap();
    let svr = hyperlocal::server::Http::new().bind(bind_path, s)?;
    svr.run()?;
    Ok(())
}


fn start_http_server_impl<S, B>(bind_target: &hyper::Uri, s: S) -> Result<(), HyperError>
where
    S: NewService<Request = Request, Response = Response<B>, Error = ::hyper::Error>
        + Send
        + Sync
        + 'static,
    B: Stream<Error = ::hyper::Error> + 'static,
    B::Item: AsRef<[u8]>,
{


    let socket_addr = format!("127.0.0.1:{}", bind_target.port().unwrap())
        .parse()
        .unwrap();

    let server = Http::new()
        .sleep_on_errors(true)
        .bind(&socket_addr, s)
        .unwrap();
    println!(
        "Listening on http://{} with 1 thread.",
        server.local_addr().unwrap()
    );
    server.run()?;

    Ok(())


}

#[derive(Clone)]
struct Srv<C> {
    downloader: Downloader<C>
}
impl<C: Connect> Srv<C> {
    fn new(downloader: Downloader<C>) -> Self {
        Srv {
            downloader: downloader
        }
    }
}

impl<C: Connect> NewService for Srv<C> {
    type Request = server::Request;
    type Response = server::Response;
    type Error = hyper::error::Error;
    type Instance = Srv<C>;

    fn new_service(&self) -> Result<Self::Instance, std::io::Error> {
        Ok(Srv {
            downloader: self.downloader.clone()
        })
    }

}
impl<C: Connect> Service for Srv<C> {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Box<Future<Item = Response, Error=hyper::Error>>;

    fn call(&self, req: Request) -> Self::Future {
        let downloaded = self.downloader.fetch_file(req.uri());

        // tokio::spawn(downloaded);

        Box::new(downloaded.and_then (move |_| {
        futures::future::ok(match (req.method(), req.path()) {
            (&hyper::Method::Get, "/") => {
                Response::new()
                    .with_header(ContentLength(PHRASE.len() as u64))
                    .with_body(PHRASE)
            },
            _ => {
                Response::new()
                    .with_status(StatusCode::NotFound)
            }
        })
    }).or_else (|_| {
        futures::future::ok(Response::new().with_status(StatusCode::InternalServerError))
    }))
    }

}

pub fn start_server<C>(_config: AppConfig, downloader: Downloader<C>) -> Result<(), io::Error>
where C: Connect {
    let srv = Srv::new(downloader);
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
            start_unix_server_impl(&_config.bind_target, srv)
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
            start_http_server_impl(&_config.bind_target, srv)
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
