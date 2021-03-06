use futures::Future;
use http::Uri;
use hyper::client::connect::Connect;
use hyper::client::connect::Connected;
use hyper::client::connect::Destination;
use std::io;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;

/// A wrapper around `Proxy`s with a connector.
#[derive(Clone, Debug)]
pub struct ProxyConnector<C> {
    proxy: Uri,
    connector: C,
}

impl<C, T: 'static> Connect for ProxyConnector<C>
where
    C: Connect<
        Error = io::Error,
        Transport = T,
        Future = Box<Future<Item = (T, Connected), Error = io::Error> + Send>,
    >,
    T: AsyncWrite + Send + AsyncRead,
{
    type Transport = T;
    type Error = io::Error;
    type Future = Box<Future<Item = (T, Connected), Error = io::Error> + Send>;

    fn connect(&self, _dst: Destination) -> Self::Future {
        Box::new(
            self.connector
                .connect(Destination::new(&self.proxy))
                .map(move |(s, c)| (s, c.proxy(true))),
        )
    }
}

impl<C> ProxyConnector<C> {
    pub fn new(connector: C, proxy: Uri) -> Result<Self, io::Error> {
        Ok(ProxyConnector {
            proxy: proxy,
            connector: connector,
        })
    }
}
