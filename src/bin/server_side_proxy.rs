extern crate clap;
extern crate futures;
extern crate hyper;
extern crate hyper_proxy;
extern crate local_cache_proxy;
extern crate lru_disk_cache;
extern crate pretty_env_logger;
extern crate tokio;
extern crate tokio_core;
extern crate tokio_uds;

use clap::{App, Arg};
use hyper::Body;
use hyper::Client;
use std::env;
#[macro_use]
extern crate log;
use local_cache_proxy::unix_socket::uri::Uri as HyperlocalUri;

use hyper::client::HttpConnector;
use local_cache_proxy::config::AppConfig;
use local_cache_proxy::net::Downloader;
use local_cache_proxy::net::ProxyConnector;
use local_cache_proxy::unix_socket::unix_connector::UnixConnector;

// The goal of this proxy is to act as an upstream of the client side proxy
// over time this will likely move from being http based to something like proto
// to add more features around intelligent fetching.
fn main() {
    pretty_env_logger::init();

    let matches = App::new("Local Bazel Cache And Proxy")
        .version("0.1")
        .about("Handles managing a local disk cache while forwarding cache misses externally")
        .author("Ian O Connell <ianoc@ianoc.net>")
        .arg(
            Arg::with_name("s3_bucket")
                .long("s3-bucket")
                .value_name("BUCKET_NAME")
                .required(true)
                .help("Bucket to hold data in")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("s3_prefix")
                .long("s3-prefix")
                .value_name("BUCKET_PREFIX")
                .required(true)
                .help("Prefix in the bucket for holding our upstream cache")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("port")
                .long("port")
                .value_name("BIND_PORT")
                .help("port number we should bind to")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("cache_folder_size")
                .long("cache-folder-size")
                .value_name("CACHE_FOLDER_SIZE")
                .help("Max size in bytes for the cache folder")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("cache_folder")
                .long("cache-folder")
                .value_name("CACHE_FOLDER")
                .help("location for the cache")
                .takes_value(true),
        )
        .get_matches();

    let cfg = AppConfig {
        proxy: None,
        upstream: matches
            .value_of("primary_upstream")
            .expect("Should never fail, expecting to see primary upstream arg")
            .parse()
            .expect("Failed to parse URI for primary upstream"),
        bind_target: format!(
            "http://0.0.0.0:{}",
            matches
                .value_of("port")
                .expect("Expected port to be specified")
        ).parse()
            .unwrap(),
        cache_folder_size: matches
            .value_of("cache_folder_size")
            .unwrap_or("32212254720")
            .parse()
            .unwrap(),
        cache_folder: matches
            .value_of("cache_folder")
            .unwrap_or(&format!(
                "{}/bazel_download_cache",
                env::home_dir().unwrap().display()
            ))
            .to_string(),
        maximum_upload_size: matches
            .value_of("maximum_upload_size")
            .unwrap_or("10485760")
            .parse()
            .unwrap(),
        maximum_download_size: matches
            .value_of("maximum_download_size")
            .unwrap_or("10485760")
            .parse()
            .unwrap(),
        idle_time_terminate: None,
    };

    // info!("setting up bazel cache folder in : {:?}", cfg.cache_folder);
    // info!("Cache folder size in : {:?}", cfg.cache_folder_size);

    // match proxy {
    //     Some(e) => {
    //         let proxy_uri = HyperlocalUri::new(e, "/").into();
    //         let connector = ProxyConnector::new(UnixConnector::new(), proxy_uri).unwrap();
    //         // The no keep alive here is super important when using a unix socket proxy
    //         // this will cause hyper to hang trying to share the connection!
    //         let http_client = Client::builder()
    //             .keep_alive(false)
    //             .build::<_, Body>(connector);
    //         let downloader = Downloader::new(&cfg).unwrap();
    //         local_cache_proxy::net::start_server(&cfg, downloader, http_client).unwrap();
    //     }
    //     None => {
    //         let http_client = Client::builder().build::<_, Body>(HttpConnector::new(4));
    //         let downloader = Downloader::new(&cfg).unwrap();
    //         local_cache_proxy::net::start_server(&cfg, downloader, http_client).unwrap();
    //     }
    // };
}
