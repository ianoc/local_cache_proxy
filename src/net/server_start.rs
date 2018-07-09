use net::server_error::ServerError;

use hyper;
use hyper::body::Payload;

use futures::Future;
use hyper::service::NewService;
use hyper::Body;
use hyper::Server;

use std::io;
use std::net::IpAddr;
use std::net::ToSocketAddrs;

fn resolve(host: &str) -> io::Result<Vec<IpAddr>> {
    (host, 0).to_socket_addrs().map(|iter| {
        iter.map(|socket_address| socket_address.ip())
            .filter(|e| e.is_ipv4())
            .collect()
    })
}

pub fn start_unix_server_impl<S, Bd>(
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

pub fn start_http_server_impl<S, Bd>(
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
    let resolved_bind_host =
        resolve(bind_target.host().unwrap()).expect("Tried to resolve hostname");

    let socket_addr = format!("{}:{}", resolved_bind_host[0], bind_target.port().unwrap())
        .parse()
        .unwrap();

    let server = Server::bind(&socket_addr)
        .serve(s)
        .map_err(|e| eprintln!("server error: {}", e));

    Ok(Box::new(server))
}
