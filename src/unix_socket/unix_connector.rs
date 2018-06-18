use hyper::client::connect::Connect;
use hyper::client::connect::Connected;
use std::clone::Clone;
use std::io;

use super::unix_stream::UnixStream;
use futures::future;
use futures::Future;
use hyper::client::connect::Destination;

use super::Uri;

const UNIX_SCHEME: &str = "unix";

/// A type which implements hyper's client connector interface
/// for unix domain sockets
///
/// `UnixConnector` instances assume uri's
/// constructued with `local_cache_proxy::Uri::new()` which produce uris with a `unix://`
/// scheme
///
/// # examples
///
/// ```no_run
/// extern crate hyper;
/// extern crate local_cache_proxy;
///
/// use hyper::Client;
/// use hyper::Body;
///    let http_client = Client::builder()
///                .keep_alive(false)
///      .build::<_, Body>(local_cache_proxy::unix_socket::unix_connector::UnixConnector::new());
/// ```
pub struct UnixConnector();

impl UnixConnector {
    pub fn new() -> Self {
        UnixConnector {}
    }
}

impl Connect for UnixConnector {
    type Transport = UnixStream;
    type Error = io::Error;
    type Future = Box<Future<Item = (UnixStream, Connected), Error = io::Error> + 'static + Send>;

    fn connect(&self, destination: Destination) -> Self::Future {
        if destination.scheme() != UNIX_SCHEME {
            return Box::new(future::err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Invalid uri {:?}", destination),
            )));
        }
        match Uri::socket_path_from_host(destination.host()) {
            Some(path) => Box::new(UnixStream::connect(path).map(|e| (e, Connected::new()))),
            _ => Box::new(future::err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Invalid uri {:?}", destination),
            ))),
        }
    }
}

impl Clone for UnixConnector {
    fn clone(&self) -> Self {
        UnixConnector()
    }
}
