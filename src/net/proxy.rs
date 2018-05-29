use hyper::{Client, Request};
// use hyper::{Chunk, Client, Request, Method, Uri};
use hyper::client::FutureResponse;
// use futures::{Future, Stream};
use hyper_proxy::{Proxy as IntProxy, ProxyConnector, Intercept};
use tokio_core::reactor::Core;
use hyperlocal::{UnixConnector, Uri as LocalUri};
use tokio_core::reactor::Handle;
use std::fmt;

pub struct Proxy {
  connector: ProxyConnector<UnixConnector>,
  core_handle: Handle
}

impl fmt::Debug for Proxy {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Proxy connector")
    }
}

impl Proxy {
      /// Create a new, empty, instance of `Shared`.
    pub fn new(core: &Core) -> Self {
      let connector = {
        let home = "sadf";
        let proxy_uri = LocalUri::new(format!("{}/.stripeproxy",home), "/").into();//".stripeproxy".parse().unwrap();
        let mut proxy = IntProxy::new(Intercept::All, proxy_uri);
        let connector = UnixConnector::new(core.handle());
        let proxy_connector = ProxyConnector::from_proxy(connector, proxy).unwrap();
        proxy_connector
      };

        Proxy {
            connector: connector,
            core_handle: core.handle()
        }
    }

    pub fn run(self, mut req: Request) -> FutureResponse {
      if let Some(headers) = self.connector.http_headers(req.uri()) {
          req.headers_mut().extend(headers.iter());
          req.set_proxy(true);
      }
      let client = Client::configure().connector(self.connector).build(&self.core_handle);

      client.request(req)
    }
}
