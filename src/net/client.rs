
use hyper::Chunk;
use futures::Poll;
use http::Response;
use hyper::client::connect::Connect;
use futures::{Future, Stream};
use hyper::client::Client;
use std::error::Error as StdError;
use hyper::Uri;
use std::fs;
use hyper::Request;
use hyper::Body;
use futures;
use tokio::prelude::*;
use std::fmt;
use http::header;

use std::time::{Duration, Instant};
use tokio::timer::Delay;

pub fn connect_for_file<C: Connect + 'static>(
    http_client: Client<C>,
    uri: Uri,
    tries: i32,
    sleep_duration: Duration,
    multiplier: u32,
) -> Box<Future<Item = Response<Body>, Error = String> + Send + 'static> {
    let http_response = http_client.request(Request::get(uri.clone()).body(Body::empty()).unwrap());
    let when = Instant::now() + Duration::from_millis(1000 * 5);
    let task = Delay::new(when);

    let timeout = task.then(|_| Err("Request timeout".to_string()));

    let ret: Box<Future<Item = Response<Body>, Error = String> + Send + 'static> = Box::new(
        http_response
            .map_err(|e| {
                warn!("Error in req: {:?}", e);
                e.description().to_string()
            })
            .select(timeout)
            .map(|(e, _)| e)
            .map_err(|(e, _)| e),
    );

    if tries > 0 {
        info!(
            "Going to preform retry fetching from remote cache, {} tries left",
            tries
        );
        Box::new(ret.or_else(move |_| {
            Delay::new(Instant::now() + sleep_duration)
                .map_err(|_| "timeout error".to_string())
                .and_then(move |_| {
                    connect_for_file(
                        http_client,
                        uri,
                        tries - 1,
                        sleep_duration * multiplier,
                        multiplier,
                    )
                })
        }))
    } else {
        ret
    }
}


pub fn connect_for_head<C: Connect + 'static>(
    http_client: Client<C>,
    uri: Uri,
    tries: i32,
    sleep_duration: Duration,
    multiplier: u32,
) -> Box<Future<Item = Option<u64>, Error = String> + Send + 'static> {
    let http_response =
        http_client.request(Request::head(uri.clone()).body(Body::empty()).unwrap());
    let when = Instant::now() + Duration::from_millis(1000 * 5);
    let task = Delay::new(when);

    let timeout = task.then(|_| Err("Request timeout".to_string()));

    let ret: Box<Future<Item = Option<u64>, Error = String> + Send + 'static> = Box::new(
        http_response
            .map_err(|e| {
                warn!("Error in req: {:?}", e);
                e.description().to_string()
            })
            .select(timeout)
            .map(|(res, _)| match res.headers()
                .get(header::CONTENT_LENGTH)
                .map(|e| e.to_str()) {
                Some(Ok(e)) => e.parse().map(Some).unwrap_or(None),
                _ => None,
            })
            .map_err(|(e, _)| e),
    );

    if tries > 0 {
        info!(
            "Going to preform retry fetching from remote cache, {} tries left",
            tries
        );
        Box::new(ret.or_else(move |_| {
            Delay::new(Instant::now() + sleep_duration)
                .map_err(|_| "timeout error".to_string())
                .and_then(move |_| {
                    connect_for_head(
                        http_client,
                        uri,
                        tries - 1,
                        sleep_duration * multiplier,
                        multiplier,
                    )
                })
        }))
    } else {
        ret
    }
}

pub(super) fn path_exists(path: &::std::path::PathBuf) -> bool {
    match fs::metadata(path) {
        Ok(metadata) => metadata.is_file(),
        Err(_) => false,
    }
}


#[derive(Debug)]
pub enum BodyStreamerError {
    TimeoutError,
    HyperError(::hyper::Error),
}


impl StdError for BodyStreamerError {
    fn description(&self) -> &str {
        match self {
            BodyStreamerError::TimeoutError => "Timeout making request",
            BodyStreamerError::HyperError(e) => e.description(),
        }
    }
}

impl fmt::Display for BodyStreamerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.description())
    }
}
impl From<::hyper::Error> for BodyStreamerError {
    fn from(error: ::hyper::Error) -> Self {
        BodyStreamerError::HyperError(error)
    }
}


pub(super) struct BodyStreamer(Body, Instant);
impl BodyStreamer {
    pub fn new(body: Body) -> Self {
        BodyStreamer(body, Instant::now())
    }
}
impl Stream for BodyStreamer {
    type Error = BodyStreamerError;
    type Item = Chunk;
    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        let time_since_last_data = Instant::now() - self.1;

        // Notify to ensure we can timeout
        futures::task::current().notify();

        if time_since_last_data > Duration::from_millis(1000 * 3) {
            return Err(BodyStreamerError::TimeoutError);
        }

        let poll_result = try_ready!(self.0.poll());

        self.1 = Instant::now();
        Ok(Async::Ready(poll_result))
    }
}
