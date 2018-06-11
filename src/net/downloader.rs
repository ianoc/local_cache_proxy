use config::AppConfig;
use futures;
use futures::future::Either;
use futures::{Future, Stream};
use http::header;
use hyper::client::connect::Connect;
use hyper::client::Client;
use hyper::Request;
use hyper::Uri;
use hyper::{Body, StatusCode};
use lru_disk_cache::LruDiskCache;
use net::client::connect_for_file;
use net::client::path_exists;
use net::client::BodyStreamer;
use rand;
use std::error::Error as StdError;
use std::fmt;
use std::fs;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tempdir::TempDir;
use tokio::timer::Delay;

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

fn get_content_length(headers: &header::HeaderMap) -> Option<u64> {
    let header_value = match headers.get(header::CONTENT_LENGTH) {
        None => return None,
        Some(e) => e,
    };

    let header_string = match header_value.to_str() {
        Err(_) => return None,
        Ok(e) => e,
    };

    let header_size: u64 = match header_string.parse() {
        Err(_) => return None,
        Ok(e) => e,
    };

    Some(header_size)
}

fn internal_fetch_file_with_retries<C: Connect + 'static>(
    maximum_download_size: u64,
    download_root: String,
    http_client: Client<C>,
    uri: Uri,
    tries: i32,
    sleep_duration: Duration,
    multiplier: u32,
) -> Box<Future<Item = Option<(u64, String)>, Error = String> + Send + 'static> {
    let req_uri = uri.clone();
    let req_uri3 = uri.clone();
    let req_uri4 = uri.clone();

    let next_download_root = download_root.clone();

    let initial_file_response = connect_for_file(
        http_client.clone(),
        uri.clone(),
        1,
        Duration::from_millis(500),
        1,
    );

    let fetch_fut = Box::new(initial_file_response.and_then(move |res| {
        debug!(
            "Got res back from req: {:?} with length: {:?}",
            req_uri,
            res.headers().get(header::CONTENT_LENGTH)
        );
        let content_length: Option<u64> = get_content_length(res.headers());
        let should_download = match content_length {
            None => {
                warn!("Content length not found for query");
                false
            } // TODO make this configurable?
            Some(siz) => {
                let ok_size = siz <= maximum_download_size;
                if !ok_size {
                    info!(
                        "Skipping download for {} since too large: {}",
                        req_uri4, siz
                    );
                }
                ok_size
            }
        };
        match (should_download, res.status()) {
            (true, StatusCode::OK) => {
                let temp_file_name: u64 = rand::random();

                let file_path =
                    ::std::path::Path::new(&download_root).join(temp_file_name.to_string());

                let mut file = fs::File::create(&file_path).unwrap();
                Either::B(
                    BodyStreamer::new(res.into_body())
                        .for_each(move |chunk| {
                            file.write_all(&chunk).map_err(|e| {
                                warn!("example expects stdout is open, error={}", e);
                                panic!("example expects stdout is open, error={}", e)
                            })
                        })
                        .map(move |_| {
                            Some((
                                content_length.unwrap(),
                                file_path.to_string_lossy().to_string(),
                            ))
                        })
                        .map_err(|e| e.to_string()),
                )
            }
            _ => Either::A(futures::future::ok(None)),
        }
    }));

    if tries > 0 {
        Box::new(fetch_fut.or_else(move |_| {
            Delay::new(Instant::now() + sleep_duration)
                .map_err(|_| "timeout error".to_string())
                .and_then(move |_| {
                    info!(
                        "Going to preform retry fetching {} from remote cache, {} tries left",
                        req_uri3, tries
                    );
                    internal_fetch_file_with_retries(
                        maximum_download_size,
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
        let file_path = tmp_download_root
            .lock()
            .unwrap()
            .path()
            .join(file_name.clone());

        let upload_path = ::std::path::Path::new(&self.config.cache_folder).join(&file_name);

        let lru_cache_copy = Arc::clone(&self.lru_cache);

        let mut file = fs::File::create(&file_path).unwrap();

        Box::new(
            req.into_body()
                .for_each(move |chunk| {
                    file.write_all(&chunk)
                        .map_err(|e| panic!("example expects stdout is open, error={}", e))
                })
                .map(move |_e| {
                    let mut lru_cache = lru_cache_copy
                        .lock()
                        .map_err(|e| {
                            error!(
                                "Fail access lru cache for name: {}, path: {:?}, error: {:?}",
                                file_name, file_path, e
                            );
                            e
                        })
                        .unwrap();
                    if !path_exists(&upload_path) {
                        lru_cache.insert_file(&file_name, file_path).unwrap();
                        Some(file_name)
                    } else {
                        None
                    }
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
    ) -> Box<Future<Item = Option<u64>, Error = String> + Send> {
        debug!("Querying for uri: {:?}", uri);

        let tmp_download_root = &self.tmp_download_root;
        let file_name = file_name.clone();

        let download_root = {
            tmp_download_root
                .lock()
                .unwrap()
                .path()
                .clone()
                .to_string_lossy()
                .to_string()
        };

        let fetched_fut = internal_fetch_file_with_retries(
            self.config.maximum_download_size,
            download_root,
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
                    debug!("Finished operating on uri: {:?}", req_uri3);
                    match file_path_opt {
                        None => Ok(None),
                        Some((file_size, file_path)) => {
                            let mut lru_cache = lru_cache_copy.lock().unwrap();
                            lru_cache
                                .insert_file(&file_name, file_path)
                                .map_err(|e| e.description().to_string())
                                .map(move |_| Some(file_size))
                        }
                    }
                })
                .and_then(|e| futures::future::result(e))
                .map_err(move |e| {
                    warn!(
                        "Failed operating on uri: {:?}, with error: {:?}",
                        req_uri4, e
                    );
                    e.to_string()
                }),
        )
    }
}
