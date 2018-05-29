
use hyper::client::FutureResponse;
use hyper::Error;
use hyper::client::Connect;
use futures::{Future, Stream};
use hyper::client::Client;
use tempdir::TempDir;
use std::io;
use lru_disk_cache::LruDiskCache;
use std::sync::{Arc, Mutex};
use std::fmt;
use config::AppConfig;
use std::error::Error as StdError;
use hyper::Uri;
use std::fs;
use flate2::read::GzDecoder;
use hyper::{Request, Response};
use hyper::Method;
use hyper;

pub struct Downloader<C> {
  pub tmp_download_root: Option<TempDir>,
  pub lru_cache: Arc<Mutex<LruDiskCache>>,
  pub http_client: Arc<Mutex<C>>
}

impl<C> fmt::Debug for Downloader<C> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Downloader")
    }
}

impl<C> Drop for Downloader<C> {
    fn drop(&mut self) {
        if let Some(downloader) = self.tmp_download_root.take() {
            // We are going to do this on exit, if we can't clean up ignore it.
            downloader.close().unwrap_or_default();
        }
    }
}


impl<C, B> Downloader<Client<C,B>>
where C: Connect,
      B: Stream<Error=hyper::Error> + 'static,
      B::Item: AsRef<[u8]>,
      {

      /// Create a new, empty, instance of `Shared`.
    pub fn new(app_config: AppConfig, http_client: Client<C, B>) -> Result<Self, Box<StdError>> {
        let dir = TempDir::new("local_cache_proxy")?;
    let _file_path = dir.path().join("foo.txt");
    let cache = LruDiskCache::new(app_config.cache_folder, 1)?;

    Ok(Downloader {
        tmp_download_root: Some(dir),
        lru_cache: Arc::new(Mutex::new(cache)),
        http_client: Arc::new(Mutex::new(http_client))
    })

    }


    // // Connecting to http will trigger regular GETs and POSTs.
    // // We need to manually append the relevant headers to the request
    // let uri: Uri = "http://asdfasdfsda/ianoc/16233".parse().unwrap();
    // let req = Request::new(Method::Get, uri.clone());

    // let fut_http = proxy.run(req)
    //     .and_then(|res| res.body().concat2())
    //     .map(move |body: Chunk| ::std::str::from_utf8(&body).unwrap().to_string());

    // let _http_res = core.run(fut_http).unwrap();


    pub fn fetch_file(downloader: Self, uri: & Uri, _download_path: &str) -> FutureResponse {
    //         let req = Request::new(Method::Get, uri.clone());
    //         let tmp_download_root = &downloader.tmp_download_root.as_ref().unwrap();
    //     let file_path = tmp_download_root.path().join(uri.path().replace("/", "__"));

    //         let http_client = downloader.http_client.lock().unwrap();
    //         http_client.request(req)
    //             .and_then(|res|{



    //         let mut file = fs::File::create(&file_path)?;
    //         let mut deflate = GzDecoder::new(response);

    // })
    //         let path = uri.path();
    //         let file_path = tmp_download_root.path().join(path.replace("/", "__"));

    //         let mut file = fs::File::create(&file_path)?;
    //         let mut deflate = GzDecoder::new(response);
    //         });

    //     let mut buf = [0; 128 * 1024];
    //     let mut written = 0;
    //     loop {
    //         status_line! ("icecat_fetch] " (url) ": " (written / 1024 / 1024) " MiB.");
    //         let len = match deflate.read(&mut buf) {
    //             Ok(0) => break,  // EOF.
    //             Ok(len) => len,
    //             Err(ref err) if err.kind() == io::ErrorKind::Interrupted => continue,
    //             Err(err) => return ERR!("{}: Download failed: {}", url, err),
    //         };
    //         try_s!(file.write_all(&buf[..len]));
    //         written += len;
    //     }

    // try_s!(fs::rename(tmp_path, target_path));
    // status_line_clear();
    FutureResponse(Box::new(future::err(::Error::Method)))
    }
}
