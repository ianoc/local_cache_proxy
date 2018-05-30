
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
use hyper;
use std::io::Write;
use http::header::CONTENT_LENGTH;
use hyper::Body;

pub trait HasHttpClient<C>
where
    C: Connect,
{
    fn build_http_client(&self) -> Client<C, hyper::Body>;
}


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
    // type Response = Response;
    // type Future = Box<Future<Item = Self::Response, Error = String>>;


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

        info!("Tmp download file f=path: {:?}", file_path);

        let http_response = http_client.request(req);

        let lru_cache_copy = Arc::clone(&self.lru_cache);


        Box::new(
            http_response
                .and_then(move |res| {
                    println!("{:?}", res.headers().get(CONTENT_LENGTH));
                    let mut file = fs::File::create(&file_path).unwrap();


                    res.into_body()
                        .for_each(move |chunk| {

                            file.write_all(&chunk).map_err(|e| {
                                panic!("example expects stdout is open, error={}", e)
                            })
                            //.map_err(From::from)
                        })
                        .map(move |_e| {
                            warn!(
                                "{:?} -- file_path: {:?}, file_name: {:?}",
                                _e,
                                file_path,
                                file_name
                            );
                            {
                                let mut lru_cache = lru_cache_copy.lock().unwrap();
                                lru_cache.insert_file(&file_name, file_path).unwrap();
                            }
                            Some(file_name)
                        })

                })
                .map_err(|e| e.to_string()),
        )

    }
}
