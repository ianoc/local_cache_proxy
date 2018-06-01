
use net::client::BodyStreamer;
use net::client::path_exists;
use net::client::connect_for_file;
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
use bytes::Buf;
use rand;
use std::time::{Instant, Duration};
use tokio::timer::Delay;
use hyper::body::Payload;

pub struct Downloader {
    pub config: AppConfig,
    pub tmp_download_root: Arc<Mutex<TempDir>>,
    pub lru_cache: Arc<Mutex<LruDiskCache>>,
}

impl fmt::Debug for Downloader {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Downloader")
    }
}

impl Clone for Downloader {
    fn clone(&self) -> Self {
        Downloader {
            tmp_download_root: Arc::clone(&self.tmp_download_root),
            lru_cache: Arc::clone(&self.lru_cache),
            config: self.config.clone(),
        }
    }
}

// fn internal_concurrent_fetch<C: Connect + 'static>(
//     download_root: String,
//     http_client: Client<C>,
//     uri: Uri
//     ) {
//     connect_for_head
// }
fn internal_fetch_file_with_retries<C: Connect + 'static>(
    download_root: String,
    http_client: Client<C>,
    uri: Uri,
    tries: i32,
    sleep_duration: Duration,
    multiplier: u32,
) -> Box<Future<Item = Option<String>, Error = String> + Send + 'static> {

    info!("Querying for uri: {:?}", uri);

    let req_uri = uri.clone();
    let req_uri2 = uri.clone();
    let req_uri3 = uri.clone();

    let next_download_root = download_root.clone();

    let initial_file_response = connect_for_file(
        http_client.clone(),
        uri.clone(),
        1,
        Duration::from_millis(500),
        1,
    );

    let fetch_fut = Box::new(initial_file_response.and_then(move |res| {
        info!(
            "Got res back from req: {:?} with length: {:?}",
            req_uri,
            res.headers().get(header::CONTENT_LENGTH)
        );
        match res.status() {
            StatusCode::OK => {

                let temp_file_name: u64 = rand::random();

                let file_path =
                    ::std::path::Path::new(&download_root).join(temp_file_name.to_string());


                let mut file = fs::File::create(&file_path).unwrap();
                let body = res.into_body();
                warn!(
                    "Body content length: {:?} {:?}",
                    body.content_length(),
                    body
                );
                Either::B(
                    BodyStreamer::new(body)
                        .for_each(move |chunk| {
                            info!(
                                "Operating on uri: {:?} , got chunk with size {}",
                                req_uri2,
                                chunk.remaining()
                            );

                            file.write_all(&chunk).map_err(|e| {
                                warn!("example expects stdout is open, error={}", e);
                                panic!("example expects stdout is open, error={}", e)
                            })
                            //.map_err(From::from)
                        })
                        .map(move |_| Some(file_path.to_string_lossy().to_string()))
                        .map_err(|e| e.to_string()),
                )
            }
            _ => Either::A(futures::future::ok(None)),
        }
    }));

    if tries > 0 {
        info!(
            "Going to preform retry fetching {} from remote cache, {} tries left",
            req_uri3,
            tries
        );
        Box::new(fetch_fut.or_else(move |_| {
            Delay::new(Instant::now() + sleep_duration)
                .map_err(|_| "timeout error".to_string())
                .and_then(move |_| {
                    internal_fetch_file_with_retries(
                        next_download_root,
                        http_client,
                        uri,
                        tries - 1,
                        sleep_duration * multiplier,
                        multiplier,
                    )


                })
        }))
    } else {
        fetch_fut
    }
}

impl Downloader {
    /// Create a new, empty, instance of `Shared`.
    pub fn new(app_config: &AppConfig) -> Result<Self, Box<StdError>> {
        let dir = TempDir::new("local_cache_proxy")?;
        let cache = LruDiskCache::new(
            app_config.cache_folder.clone(),
            app_config.cache_folder_size,
        )?;

        Ok(Downloader {
            tmp_download_root: Arc::new(Mutex::new(dir)),
            lru_cache: Arc::new(Mutex::new(cache)),
            config: app_config.clone(),
        })
    }

    pub fn save_file(
        self: &Self,
        file_name: &String,
        req: Request<Body>,
    ) -> Box<Future<Item = Option<String>, Error = String> + Send> {
        let tmp_download_root = &self.tmp_download_root;
        let file_name = file_name.clone();
        let file_path = tmp_download_root.lock().unwrap().path().join(
            file_name.clone(),
        );

        let upload_path = ::std::path::Path::new(&self.config.cache_folder).join(&file_name);

        // Cannot action the fact we know we don't want this file here
        // why does bazel even send it?
        //
        // if we action it here then bazel will see a broken pipe/failed upload
        //
        // if path_exists(&upload_path) {
        //     return Box::new(futures::future::ok(None))
        // }

        let lru_cache_copy = Arc::clone(&self.lru_cache);

        let mut file = fs::File::create(&file_path).unwrap();

        let u = req.uri().clone();
        Box::new(
            req.into_body()
                .for_each(move |chunk| {
                    info!("Operating on uri: {:?} , got chunk", u);
                    file.write_all(&chunk).map_err(|e| {
                        panic!("example expects stdout is open, error={}", e)
                    })
                })
                .map(move |_e| {
                    {
                        let mut lru_cache = lru_cache_copy
                            .lock()
                            .map_err(|e| {
                                error!(
                                    "Fail access lru cache for name: {}, path: {:?}, error: {:?}",
                                    file_name,
                                    file_path,
                                    e
                                );
                                e
                            })
                            .unwrap();
                        if !path_exists(&upload_path) {
                            lru_cache.insert_file(&file_name, file_path).unwrap();
                        }


                    }
                    Some(file_name)
                })
                .map_err(|e| {
                    warn!("Inner downloader error: {:?}", e);
                    e.to_string()
                }),
        )

    }



    pub fn fetch_file<'a, C: Connect + 'static>(
        self: &Self,
        http_client: &Client<C>,
        uri: &Uri,
        file_name: &String,
    ) -> Box<Future<Item = Option<String>, Error = String> + Send> {

        info!("Querying for uri: {:?}", uri);

        let tmp_download_root = &self.tmp_download_root;
        let file_name = file_name.clone();

        let fetched_fut = internal_fetch_file_with_retries(
            tmp_download_root
                .lock()
                .unwrap()
                .path()
                .clone()
                .to_string_lossy()
                .to_string(),
            http_client.clone(),
            uri.clone(),
            3,
            Duration::from_millis(20000),
            2,
        );

        let lru_cache_copy = Arc::clone(&self.lru_cache);
        let req_uri3 = uri.clone();
        let req_uri4 = uri.clone();

        Box::new(
            fetched_fut
                .map(move |file_path_opt| {
                    info!("Finished operating on uri: {:?}", req_uri3);
                    match file_path_opt {
                        None => Ok(None),
                        Some(file_path) => {
                            let mut lru_cache = lru_cache_copy.lock().unwrap();
                            lru_cache
                                .insert_file(&file_name, file_path)
                                .map_err(|e| e.description().to_string())
                                .map(move |_| Some(file_name))
                        }
                    }
                })
                .and_then(|e| futures::future::result(e))
                .map_err(move |e| {
                    warn!(
                        "Failed operating on uri: {:?}, with error: {:?}",
                        req_uri4,
                        e
                    );
                    e.to_string()
                }),
        )

    }
}
