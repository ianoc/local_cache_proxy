use config::S3Config;
use futures::Stream;
use net::proxy_request::ProxyRequest;
use net::server_error::ServerError;
use net::server_io::empty_with_status_code;
use net::server_io::empty_with_status_code_fut;
use net::server_io::send_file;
use rusoto_s3::GetObjectRequest;
use std::io::Read;
use std::io::Write;

use net::server_start::start_http_server_impl;
use net::server_start::start_unix_server_impl;
use net::state::State;
use rusoto_s3::PutObjectRequest;
use std::fs::File;
use std::time::Duration;

use hyper::Client;

use config::AppConfig;
use hyper;
use hyper::body::Payload;
use hyper::service::service_fn;
use std::io;

use futures;
use futures::Future;
use hyper::client::connect::Connect;
use hyper::service::NewService;
use hyper::Server;
use hyper::Uri as HyperUri;
use hyper::{Body, Method, Request, Response, StatusCode};
use net::background_uploader::RequestUpload;
use net::downloader::Downloader;
use net::process_action_cache::{process_action_cache_response, process_existing_action_caches};
use rusoto_s3::{GetObjectError, S3, S3Client};
use std::error::Error;
use std::fs;
use std::path::Path;
use std::result::Result;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

type ResponseFuture = Box<Future<Item = Response<Body>, Error = ServerError> + Send>;

fn current_file_size(path: &str) -> Option<u64> {
    match fs::metadata(path) {
        Ok(_meta) => Some(_meta.len()), // file is present
        Err(_e) => None,
    }
}

// using from https://github.com/stephank/hyper-staticfile/blob/master/src/static_service.rs

fn duration_to_float_seconds(d: Duration) -> f64 {
    let f: f64 = d.subsec_nanos() as f64 / 1_000_000_000_0.0;
    f + d.as_secs() as f64
}

fn get_object_from_s3_with_file_name(
    client: &S3Client,
    bucket: &str,
    prefix: &str,
    local_filename: &Path,
) -> Box<Future<Item = Option<u64>, Error = String> + Send + 'static> {
    info!("Issuing request to s3://{}/{}", bucket, prefix);
    let get_req = GetObjectRequest {
        bucket: bucket.to_owned(),
        key: prefix.to_owned(),
        ..Default::default()
    };

    match client.get_object(&get_req).sync() {
        Err(GetObjectError::NoSuchKey(_)) => Box::new(futures::future::ok(None)),
        Err(o) => {
            warn!("Unknown other error {:?}", o);
            Box::new(futures::future::ok(None))
        }

        Ok(result) => {
            let mut f = File::create(local_filename).unwrap();
            let total_size: Option<u64> = result.content_length.map(|e| e as u64);

            println!("get object result: {:#?}", result);

            let stream = result.body.unwrap();
            Box::new(
                stream
                    .for_each(move |e| f.write_all(&e).map(|_| ()).map_err(From::from))
                    .map(move |_| total_size)
                    .map_err(|e| e.to_string()),
            )
        }
    }
}

fn put_object_with_file_name(
    client: &S3Client,
    bucket: &str,
    dest_filename: &str,
    local_filename: &Path,
) -> Result<(), String> {
    let mut f = File::open(local_filename).unwrap();
    let mut contents: Vec<u8> = Vec::new();
    match f.read_to_end(&mut contents) {
        Err(why) => return Err(format!("Error opening file to send to S3: {}", why)),
        Ok(_) => {
            let req = PutObjectRequest {
                bucket: bucket.to_owned(),
                key: dest_filename.to_owned(),
                body: Some(contents.into()),
                ..Default::default()
            };
            client
                .put_object(&req)
                .sync()
                .map(|_| ())
                .map_err(|e| e.to_string())?
        }
    }
    Ok(())
}

fn to_upstream_path(proxy_request: &ProxyRequest, s3_config: &S3Config) -> String {
    format!(
        "{}/{}/repo_{}/{}",
        s3_config.prefix, proxy_request.tpe, proxy_request.repo, proxy_request.digest
    )
}
fn get_request(
    instant: Instant,
    req: Request<Body>,
    s3_client: Arc<S3Client>,
    config: &AppConfig,
    s3_config: &S3Config,
) -> ResponseFuture {
    info!("Start Get request to {:?}", req.uri());
    let proxy_request = ProxyRequest::new(req.uri());
    let file_name = proxy_request.file_name();

    let data_source_path = Path::new(&config.cache_folder).join(&file_name);
    let path = req.uri().path().to_string().clone();

    let s3_cfg2 = s3_config.clone();

    let upstream_path = to_upstream_path(&proxy_request, s3_config);

    let inner_s3_client = Arc::clone(&s3_client);
    Box::new(
        futures::done(Ok(upstream_path)).and_then(move |prefix_uri| {
            let downloaded_file_future: Box<
                Future<Item = Option<u64>, Error = ServerError> + Send,
            > = match current_file_size(data_source_path.to_str().unwrap()) {
                None => Box::new(
                    get_object_from_s3_with_file_name(
                        &inner_s3_client,
                        &s3_cfg2.bucket,
                        &prefix_uri,
                        &data_source_path,
                    ).map_err(From::from),
                ),
                Some(len) => Box::new(futures::future::ok(Some(len))),
            };

            let req_uri_string = prefix_uri.clone();

            let downloaded_fut = downloaded_file_future
                .and_then(move |file_path| {
                    // info!("Get request issued to : {} --> {:?}", req.uri(), file_path);
                    match file_path {
                        Some(file_len) => {
                            if instant.elapsed().as_secs() >= 2 {
                                info!(
                                    "[{:?}] took: {} seconds at {} MB/sec",
                                    path,
                                    duration_to_float_seconds(instant.elapsed()),
                                    (file_len as f64 / 1_000_000 as f64)
                                        / duration_to_float_seconds(instant.elapsed())
                                );
                            }
                            send_file(data_source_path.to_str().unwrap().to_string())
                        }
                        None => {
                            let mut res = Response::new(Body::empty());
                            *res.status_mut() = StatusCode::NOT_FOUND;
                            Box::new(futures::future::ok(res))
                        }
                    }
                })
                .or_else(move |o| {
                    warn!(
                        "Error: {:?} after {} seconds",
                        o,
                        instant.elapsed().as_secs()
                    );
                    let e: ResponseFuture = Box::new(futures::future::ok({
                        let mut res = Response::new(
                            format!("Requested uri: {:?}\n{:?}", req_uri_string, o).into(),
                        );
                        *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                        res
                    }));
                    e
                });

            downloaded_fut
        }),
    )
}

fn upstream_upload(
    s3_client: &S3Client,
    s3_config: &S3Config,
    proxy_request: &ProxyRequest,
    file_path: &Path,
) {
    let upstream_path = to_upstream_path(&proxy_request, s3_config);
    put_object_with_file_name(s3_client, &s3_config.bucket, &upstream_path, file_path).unwrap()
}

fn put_request(
    instant: Instant,
    req: Request<Body>,
    s3_client: Arc<S3Client>,
    downloader: &Downloader,
    config: &AppConfig,
    s3_config: &S3Config,
) -> ResponseFuture {
    let proxy_request = ProxyRequest::new(req.uri());
    let file_name = proxy_request.file_name();

    info!("Put request: {:?}", req.uri().path());

    let path = req.uri().path().to_string().clone();

    let upload_path = Path::new(&config.cache_folder).join(&file_name);

    let processor_config = config.clone();
    let s3_cfg = s3_config.clone();
    Box::new(
        downloader
            .save_file(&file_name, req)
            .map(move |_file| {
                match _file {
                    Some(_f) => {
                        process_action_cache_response(&processor_config, &_f)
                            .map_err(|e| {
                                warn!(
                                    "[Put]Failed to process action cache with: {:?} for {:?}",
                                    e, _f
                                );
                                ()
                            })
                            .unwrap_or(());

                        upstream_upload(&s3_client, &s3_cfg, &proxy_request, &upload_path);
                        ()
                    }
                    None => (),
                };

                if instant.elapsed().as_secs() > 60 {
                    info!(
                        "Put request to {:?} took {} seconds",
                        path,
                        instant.elapsed().as_secs()
                    );
                }
                empty_with_status_code(StatusCode::CREATED)
            })
            .map_err(|e| {
                warn!("Error doing put! {:?}", e);
                From::from(e)
            }),
    )
}

pub fn start_server(
    config: &AppConfig,
    s3_config: &S3Config,
    raw_s3_client: S3Client,
) -> Result<(), io::Error> {
    let s = Arc::new(Mutex::new(State {
        last_user_facing_request: Instant::now(),
        last_background_upload: Instant::now(),
    }));

    process_existing_action_caches(config.clone());

    let cfg = config.clone();
    let s3_cfg = s3_config.clone();
    let s3_client = Arc::new(raw_s3_client);
    let downloader = Downloader::new(&cfg).unwrap();

    let new_service = move || {
        // Move a clone of `client` into the `service_fn`.
        let inner_s3_client = Arc::clone(&s3_client);
        let state = Arc::clone(&s);

        let inner_cfg = cfg.clone();
        let inner_s3_cfg = s3_cfg.clone();
        let inner_downloader = downloader.clone();
        service_fn(move |req| {
            {
                let mut locked = state.lock().unwrap();
                locked.last_user_facing_request = Instant::now();
            }
            info!("{:?}", req);
            Box::new(
                match req.method() {
                    &Method::GET => get_request(
                        Instant::now(),
                        req,
                        Arc::clone(&inner_s3_client),
                        &inner_cfg.clone(),
                        &inner_s3_cfg.clone(),
                    ),
                    &Method::PUT => put_request(
                        Instant::now(),
                        req,
                        Arc::clone(&inner_s3_client),
                        &inner_downloader,
                        &inner_cfg.clone(),
                        &inner_s3_cfg.clone(),
                    ),
                    _ => {
                        info!(
                            "Attempted {:?} operation to {:?}",
                            req.method(),
                            req.uri().path()
                        );
                        Box::new(empty_with_status_code_fut(
                            StatusCode::INTERNAL_SERVER_ERROR,
                        ))
                    }
                }.map_err(|e| {
                    error!("Ran into error: {}", e.description());
                    e
                }),
            )
        })
    };

    let server_engine = {
        info!(
            "Going to bind/start server on http path for: 0.0.0.0:{}",
            config.bind_target.port().unwrap()
        );
        start_http_server_impl(&config.bind_target, new_service).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Error configuring unix server: {:?}", e),
            )
        })?
    };

    hyper::rt::run(server_engine.map(|_| ()).map_err(|_| ()));
    return Ok(());
}
