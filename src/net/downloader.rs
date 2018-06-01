
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
use std::time::Duration;
use bytes::Buf;

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
        let file_path = tmp_download_root.lock().unwrap().path().join(
            file_name.clone(),
        );

        let lru_cache_copy = Arc::clone(&self.lru_cache);
        let req_uri = uri.clone();
        let req_uri2 = uri.clone();
        let req_uri3 = uri.clone();
        let req_uri4 = uri.clone();

        Box::new(
            connect_for_file(
                http_client.clone(),
                uri.clone(),
                3,
                Duration::from_millis(500),
                2,
            ).and_then(move |res| {
                info!(
                    "Got res back from req: {:?} with length: {:?}",
                    req_uri,
                    res.headers().get(header::CONTENT_LENGTH)
                );
                match res.status() {
                    StatusCode::OK => {
                        let mut file = fs::File::create(&file_path).unwrap();
                        Either::B(
                            res.into_body()
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
                                .map(move |_e| {
                                    info!("Finished operating on uri: {:?}", req_uri3);
                                    let mut lru_cache = lru_cache_copy.lock().unwrap();
                                    lru_cache
                                        .insert_file(&file_name, file_path)
                                        .map_err(|e| e.description().to_string())
                                        .map(move |_| Some(file_name))
                                })
                                .map_err(move |e| {
                                    warn!(
                                        "Failed operating on uri: {:?}, with error: {:?}",
                                        req_uri4,
                                        e
                                    );
                                    e.description().to_string()
                                })
                                .flatten(),
                        )
                    }
                    _ => Either::A(futures::future::ok(None)),
                }

            })
                .map_err(|e| e.to_string()),
        )

    }
}
