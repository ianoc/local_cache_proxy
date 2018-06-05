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
/// constructued with `hyperlocal::Uri::new()` which produce uris with a `unix://`
/// scheme
///
/// # examples
///
/// ```no_run
/// extern crate hyper;
/// extern crate hyperlocal;
/// extern crate tokio_core;
///
/// let core = tokio_core::reactor::Core::new().unwrap();
/// let client = hyper::Client::configure()
///    .connector(
///      hyperlocal::UnixConnector::new(core.handle())
///    )
///    .build(&core.handle());
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
