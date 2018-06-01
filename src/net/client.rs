
use hyper::Chunk;
use futures::Poll;
use http::Response;
use hyper::client::connect::Connect;
use futures::{Future, Stream};
use hyper::client::Client;
use tempdir::TempDir;
use lru_disk_cache::LruDiskCache;
use std::sync::{Arc, Mutex};
use std::fmt;
use config::AppConfig;
use std::error::Error as StdError;
use hyper::Uri;
use std::fs;
use hyper::Request;
use std::io::Write;
use hyper::{Body, StatusCode};
use futures;
use futures::future::Either;
use http::header;
use std::time::{Duration, Instant};
use tokio::timer::Delay;
use bytes::Buf;

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

pub(super) fn path_exists(path: &::std::path::PathBuf) -> bool {
    match fs::metadata(path) {
        Ok(metadata) => metadata.is_file(),
        Err(_) => false,
    }
}

struct BodyStreamer(Body);
impl Stream for BodyStreamer {
    type Error = String;
    type Item = Chunk;
    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        unimplemented!()
    }
}
