extern crate hyper;
extern crate hyper_proxy;
extern crate futures;
extern crate tokio_core;
extern crate hyperlocal;
extern crate lru_disk_cache;
extern crate clap;
extern crate local_cache_proxy;
extern crate pretty_env_logger;
extern crate tokio;

use hyper::client::Client;
use clap::{App, Arg};
use std::env;


// use std::{thread, time};

use local_cache_proxy::config::AppConfig;
use local_cache_proxy::net::Downloader;


fn main() {
    // let proxy = Proxy::new(&core);

    // // Connecting to http will trigger regular GETs and POSTs.
    // // We need to manually append the relevant headers to the request
    // let uri: Uri = "http://asdfasdfsda/ianoc/16233".parse().unwrap();
    // let req = Request::new(Method::Get, uri.clone());

    // let fut_http = proxy.run(req)
    //     .and_then(|res| res.body().concat2())
    //     .map(move |body: Chunk| ::std::str::from_utf8(&body).unwrap().to_string());

    // let _http_res = core.run(fut_http).unwrap();
    // println!("{}", _http_res);

    // LruDiskCache::new("/tmp/cache_folder", 1).unwrap();
    // thread::sleep(time::Duration::from_millis(60*1000));

    // run().unwrap();

    pretty_env_logger::init();

    let matches = App::new("Local Bazel Cache And Proxy")
        .version("0.1")
        .about(
            "Handles managing a local disk cache while forwarding cache misses externally",
        )
        .author("Ian O Connell <ianoc@ianoc.net>")
        .arg(
            Arg::with_name("proxy")
                .short("p")
                .long("proxy")
                .value_name("PROXY")
                .help("Outbound proxy to use, either http://<> or unix://<>")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("bind_target")
                .short("b")
                .long("bind-target")
                .value_name("BIND_TARGET")
                .help(
                    "Where we should bind to, either a unix://<path> or http://<ip/host>:<port>",
                )
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

    let proxy: Option<&str> = matches.value_of("proxy");

    let cfg = AppConfig {
        proxy: proxy.map(|e| e.to_string()),
        bind_target: matches
            .value_of("bind_target")
            .unwrap_or("http://localhost:10487")
            .parse()
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
    };

    match proxy {
        Some(x) => println!("Proxy set to : {}", x),
        None => println!("No proxy set"),
    };

    let downloader = Downloader::new(&cfg).unwrap();

    // core.run(downloader.fetch_file(
    //     &"http://www.google.com".parse().unwrap()
    // )).unwrap();
    // println!("data: {:?}", cfg);

    local_cache_proxy::net::start_server(cfg, downloader, Client::new()).unwrap();
}
