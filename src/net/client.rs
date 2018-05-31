
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

pub struct Downloader {
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
        })
    }

    pub fn save_file(
        self: &Self,
        file_name: &String,
        req: Request<Body>,
    ) -> Box<Future<Item = String, Error = String> + Send> {
        let tmp_download_root = &self.tmp_download_root;
        let file_name = file_name.clone();
        let file_path = tmp_download_root.lock().unwrap().path().join(
            file_name.clone(),
        );

        let lru_cache_copy = Arc::clone(&self.lru_cache);

        let mut file = fs::File::create(&file_path).unwrap();

        Box::new(
            req.into_body()
                .for_each(move |chunk| {

                    file.write_all(&chunk).map_err(|e| {
                        panic!("example expects stdout is open, error={}", e)
                    })
                    //.map_err(From::from)
                })
                .map(move |_e| {
                    {
                        let mut lru_cache = lru_cache_copy.lock().map_err(|e| {
                            error!("Unable to access lru cache!  for file_name: {}, file_path : {:?}, error: {:?}", file_name, file_path, e);
                            e
                        }).unwrap();
                        lru_cache.insert_file(&file_name, file_path).unwrap();
                    }
                    file_name
                })
                .map_err(|e| e.to_string()),
        )

    }

    pub fn fetch_file<C: Connect + 'static>(
        self: &Self,
        http_client: &Client<C>,
        uri: &Uri,
        file_name: &String,
    ) -> Box<Future<Item = Option<String>, Error = String> + Send> {

        info!("Querying for uri: {:?}", uri);

        let req = Request::get(uri.clone()).body(Body::empty()).unwrap();
        let tmp_download_root = &self.tmp_download_root;
        let file_name = file_name.clone();
        let file_path = tmp_download_root.lock().unwrap().path().join(
            file_name.clone(),
        );

        let http_response = http_client.request(req);

        let lru_cache_copy = Arc::clone(&self.lru_cache);


        Box::new(
            http_response
                .map_err(|e| {
                    warn!("Error in req: {:?}", e);
                    e.description().to_string()
                })
                .and_then(move |res| {
                    info!(
                        "Got res back : {:?} with length: {:?}",
                        res,
                        res.headers().get(header::CONTENT_LENGTH)
                    );
                    match res.status() {
                        StatusCode::OK => {
                            let mut file = fs::File::create(&file_path).unwrap();
                            Either::B(
                                res.into_body()
                                    .for_each(move |chunk| {

                                        file.write_all(&chunk).map_err(|e| {
                                            panic!("example expects stdout is open, error={}", e)
                                        })
                                        //.map_err(From::from)
                                    })
                                    .map(move |_e| {
                                        let mut lru_cache = lru_cache_copy.lock().unwrap();
                                        lru_cache
                                            .insert_file(&file_name, file_path)
                                            .map_err(|e| e.description().to_string())
                                            .map(|_| Some(file_name))
                                    })
                                    .map_err(|e| e.description().to_string())
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
