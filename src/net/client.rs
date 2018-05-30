
use hyper::header::ContentLength;
use hyper::client::Connect;
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
use hyper::Method;
use hyper;
use std::io::Write;

pub trait HasHttpClient<C> where C: Connect {
    fn build_http_client(&self) -> Client<C, hyper::Body>;
}


pub struct Downloader<C> {
    pub tmp_download_root: Arc<Mutex<TempDir>>,
    pub lru_cache: Arc<Mutex<LruDiskCache>>,
    pub http_client_builder: Arc<Mutex<Box<HasHttpClient<C> + Send + Sync + 'static>>>,
}

impl<C> fmt::Debug for Downloader<C> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Downloader")
    }
}

impl<C> Clone for Downloader<C> {
    fn clone(&self) -> Self {
        Downloader {
            tmp_download_root: Arc::clone(&self.tmp_download_root),
            lru_cache: Arc::clone(&self.lru_cache),
            http_client_builder: Arc::clone(&self.http_client_builder)
        }
    }
}



impl<C> Downloader<C>
where
    C: Connect,
{
    // type Response = Response;
    // type Future = Box<Future<Item = Self::Response, Error = String>>;


    /// Create a new, empty, instance of `Shared`.
    pub fn new(
        app_config: &AppConfig,
        http_client_builder: Box<HasHttpClient<C> + Send + Sync + 'static>,
    ) -> Result<Self, Box<StdError>> {
        let dir = TempDir::new("local_cache_proxy")?;
        let cache = LruDiskCache::new(
            app_config.cache_folder.clone(),
            app_config.cache_folder_size,
        )?;

        Ok(Downloader {
            tmp_download_root: Arc::new(Mutex::new(dir)),
            lru_cache: Arc::new(Mutex::new(cache)),
            http_client_builder: Arc::new(Mutex::new(http_client_builder)),
        })

    }

    pub fn fetch_file(
        self: &Self,
        uri: &Uri,
    ) -> Box<Future<Item = Option<String>, Error = String>> {
        let req = Request::new(Method::Get, uri.clone());
        let tmp_download_root = &self.tmp_download_root;
        let file_name = match uri.path() {
            "/" => "index.html".to_string(),
            o => o.trim_matches('/').replace("/", "__"),
        };

        let file_path = tmp_download_root.lock().unwrap().path().join(file_name.clone());

        info!("Filepath: {:?}", file_path);
        let http_client = self.http_client_builder.lock().unwrap().build_http_client();

        let http_response = http_client.request(req);

        let lru_cache_copy = Arc::clone(&self.lru_cache);


        Box::new(
            http_response
                .and_then(move |res| {
                    // Content-Length: 19321
                    println!("{:?}", res.headers().get::<ContentLength>());
                    let mut file = fs::File::create(&file_path).unwrap();


                    res.body()
                        .for_each(move |chunk| {
                            // info! ("icecat_fetch] " (url) ": " (written / 1024 / 1024) " MiB.");

                            file.write_all(&chunk).map_err(From::from)
                        })
                        .map(move |_e| {
                            warn!("{:?} -- file_path: {:?}", _e, file_path);
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
