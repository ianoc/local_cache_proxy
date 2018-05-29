use hyper::server::service_fn;
use hyper::header::{ContentType, ContentLength};
use hyper::server::NewService;
use hyper;
use futures::Stream;
use config::AppConfig;
use hyperlocal;
use std::io;
use hyper::{Error as HyperError};
use std::result::Result;
use hyper::{Body, Method, Request, Server, StatusCode};
use hyper::server::{Http, Response, const_service};
use std::fs;

const PHRASE: &'static str = "It's a Unix system. I know this.";


fn start_unix_server_impl<S, B>(bind_target: &hyper::Uri, s: S) -> Result<(), HyperError>
where
    S: NewService<Request = Request, Response = Response<B>, Error = ::hyper::Error>
        + Send
        + Sync
        + 'static,
    B: Stream<Error = ::hyper::Error> + 'static,
    B::Item: AsRef<[u8]> {
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
    B::Item: AsRef<[u8]> {


    let socket_addr = format!("127.0.0.1:{}", bind_target.port().unwrap()).parse().unwrap();

    let server = Http::new()
        .sleep_on_errors(true)
        .bind(&socket_addr, s)
        .unwrap();
    println!("Listening on http://{} with 1 thread.", server.local_addr().unwrap());
    server.run()?;

    Ok(())


}



pub fn start_server(_config: AppConfig)  -> Result<(), io::Error> {
  let hello = || {
        Ok(service_fn(|_| {
            Ok(
                Response::<hyper::Body>::new()
                    .with_header(ContentLength(PHRASE.len() as u64))
                    .with_header(ContentType::plaintext())
                    .with_body(PHRASE),
            )
        }))
    };

  match _config.bind_target.scheme() {
    Some("unix") => {
    debug!("Going to remove existing socket path: {}", _config.bind_target.authority().unwrap());

      fs::remove_file(_config.bind_target.authority().unwrap())?;
    info!("Going to bind/start server on unix socket for: {}", _config.bind_target);
    start_unix_server_impl(&_config.bind_target, hello).map_err(|e|
      io::Error::new(io::ErrorKind::Other, format!("Error configuring unix server: {:?}", e))
      )?
  },
    Some("http") => {
    info!("Going to bind/start server on http path for: 127.0.0.1:{}", _config.bind_target.port().unwrap());
    start_http_server_impl(&_config.bind_target, hello).map_err(|e|
      io::Error::new(io::ErrorKind::Other, format!("Error configuring unix server: {:?}", e))
      )?
    },

    _o => return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Invalid bind target {}, didn't understand the scheme", _config.bind_target),
            ))
  }
  return Ok(())
}
