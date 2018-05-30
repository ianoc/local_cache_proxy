// use hyper::Client;
// use hyper_proxy::{Proxy as IntProxy, ProxyConnector, Intercept};
// use tokio_core::reactor::Core;
// use hyperlocal::{UnixConnector, Uri as LocalUri};
// use std::fmt;
// use std::sync::{Arc, Mutex};

// pub struct Proxy {
//     pub client: Arc<Mutex<Client<ProxyConnector<UnixConnector>>>>,
// }

// impl fmt::Debug for Proxy {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         write!(f, "Proxy connector")
//     }
// }

// impl Proxy {
//     /// Create a new, empty, instance of `Shared`.
//     pub fn new(core: &Core) -> Self {
//         let connector =
//             {
//                 let proxy_uri = LocalUri::new(format!("{}/.stripeproxy", "/User/ianoc"), "/")
//                     .into(); //".stripeproxy".parse().unwrap();
//                 let mut proxy = IntProxy::new(Intercept::All, proxy_uri);
//                 let connector = UnixConnector::new(core.handle());
//                 let proxy_connector = ProxyConnector::from_proxy(connector, proxy).unwrap();
//                 proxy_connector
//             };

//         Proxy {
//             client: Arc::new(Mutex::new(Client::configure().connector(connector).build(
//                 &core.handle(),
//             ))),
//         }
//     }
// }
