use std::borrow::Cow;
use std::path::Path;

use hex::{FromHex, ToHex};
use hyper::Uri as HyperUri;

/// A type which implements `Into` for hyper's  `hyper::Uri` type
/// targetting unix domain sockets.
///
/// You can use this with any of
/// the HTTP factory methods on hyper's Client interface
/// and for creating requests
///
/// ```no_run
/// extern crate local_cache_proxy;
/// extern crate http;
/// extern crate hyper;
///
/// use hyper::Uri;
/// use hyper::Request;
///
/// let url: Uri = local_cache_proxy::unix_socket::Uri::new(
///   "/path/to/socket", "/urlpath?key=value"
///  ).into();
/// let req = Request::get(url);
///
#[derive(Debug)]
pub struct Uri<'a> {
    /// url path including leading slash, path, and query string
    encoded: Cow<'a, str>,
}

impl<'a> Into<HyperUri> for Uri<'a> {
    fn into(self) -> HyperUri {
        self.encoded.as_ref().parse().unwrap()
    }
}

impl<'a> Uri<'a> {
    /// Productes a new `Uri` from path to domain socket and request path.
    /// request path should include a leading slash
    pub fn new<P>(socket: P, path: &'a str) -> Self
    where
        P: AsRef<Path>,
    {
        let host = socket.as_ref().to_string_lossy().as_bytes().to_hex();
        let host_str = format!("unix://{}:0{}", host, path);
        Uri {
            encoded: Cow::Owned(host_str),
        }
    }

    pub(super) fn socket_path_from_host(host: &str) -> Option<String> {
        Some(host)
            .iter()
            .filter_map(|host| {
                Vec::from_hex(host)
                    .ok()
                    .map(|raw| String::from_utf8_lossy(&raw).into_owned())
            })
            .next()
    }
}
