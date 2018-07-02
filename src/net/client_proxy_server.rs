use net::proxy_request::ProxyRequest;
use net::server_error::ServerError;
use net::server_io::empty_with_status_code;
use net::server_io::empty_with_status_code_fut;
use net::server_io::send_file;
use net::server_start::start_http_server_impl;
use net::server_start::start_unix_server_impl;
use net::state::State;
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

fn get_request<C: Connect + 'static>(
    instant: Instant,
    req: Request<Body>,
    downloader: &Downloader,
    http_client: &Client<C>,
    config: &AppConfig,
) -> ResponseFuture {
    info!("Start Get request to {:?}", req.uri());
    let proxy_request = ProxyRequest::new(req.uri());

    let file_name = proxy_request.file_name();
    let file_name2 = file_name.clone();

    let data_source_path = Path::new(&config.cache_folder).join(&file_name);
    let data_source_path2 = data_source_path.clone();
    let downloader = downloader.clone();
    let http_client = http_client.clone();
    let path = req.uri().path().to_string().clone();

    let cfg2 = config.clone();

    if file_name.starts_with("cas__") {
        let gate_file = current_file_size(&format!("{}/enable_{}", config.cache_folder, file_name));
        let already_present = current_file_size(&format!("{}/{}", config.cache_folder, file_name));
        match already_present.or(gate_file) {
            None => {
                // file we never saw in an action cache message, pretend it doesn't exist.
                info!(
                    "Pretending target doesn't exist {:?}, returning 404",
                    file_name
                );
                let mut res = Response::new(Body::empty());
                *res.status_mut() = StatusCode::NOT_FOUND;
                return Box::new(futures::future::ok(res));
            }
            Some(_) => (),
        }
    }

    Box::new(
        futures::done(proxy_request.build_query_uri(&config.upstream())).and_then(
            move |query_uri| {
                let downloaded_file_future: Box<
                    Future<Item = Option<u64>, Error = ServerError> + Send,
                > = match current_file_size(data_source_path.to_str().unwrap()) {
                    None => Box::new(
                        downloader
                            .fetch_file(&http_client, &query_uri, &file_name)
                            .map(move |len| {
                                match len {
                                    Some(_) => {
                                        process_action_cache_response(&cfg2, &file_name2)
                                            .map_err(|e| {
                                                warn!(
                                            "[Get]Failed to process action cache with: {:?} -- {:?}",
                                            e,
                                            data_source_path2.to_str().unwrap().to_string()
                                        );
                                                ()
                                            })
                                            .unwrap_or(());
                                    }
                                    None => {}
                                }
                                len
                            })
                            .map_err(From::from),
                    ),
                    Some(len) => Box::new(futures::future::ok(Some(len))),
                };

                let req_uri_string = query_uri.clone();

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
                                // info!(
                                //    "[{:?}] took: {} seconds at inf MB/sec",
                                //    path,
                                //    duration_to_float_seconds(instant.elapsed())
                                // );
                                // We decided not to download the file, so 404 it!
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
            },
        ),
    )
}

fn upstream_upload(
    uploader: &RequestUpload,
    upstream_uri: &HyperUri,
    request: &ProxyRequest,
    upload_path: &String,
    cache_folder: String,
    file_name: String,
) {
    let uploader_uri = request.build_query_uri(upstream_uri).unwrap();

    uploader
        .upload(
            &uploader_uri,
            upload_path,
            Box::new(move || {
                if file_name.starts_with("cas__") {
                    match current_file_size(&format!("{}/enable_{}", cache_folder, file_name)) {
                        None => false,
                        Some(_) => true,
                    }
                } else {
                    true
                }
            }),
        )
        .map_err(|e| {
            warn!("Failed to trigger uploader!: {:?}", e);
            ()
        })
        .unwrap();
}

fn put_request(
    instant: Instant,
    req: Request<Body>,
    downloader: &Downloader,
    config: &AppConfig,
    request_upload: &RequestUpload,
) -> ResponseFuture {
    let proxy_request = ProxyRequest::new(req.uri());
    let file_name = proxy_request.file_name();

    info!("Put request: {:?}", req.uri().path());

    let upstream_uri = config.upstream();

    let path = req.uri().path().to_string().clone();

    let upload_path = Path::new(&config.cache_folder)
        .join(&file_name)
        .to_str()
        .unwrap()
        .to_string();

    let uploader = request_upload.clone();
    let cache_folder = config.cache_folder.clone();

    let processor_config = config.clone();
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

                        upstream_upload(
                            &uploader,
                            &upstream_uri,
                            &proxy_request,
                            &upload_path,
                            cache_folder.clone(),
                            file_name.clone(),
                        );
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

pub fn start_server<C: Connect + 'static>(
    config: &AppConfig,
    downloader: Downloader,
    http_client: Client<C>,
) -> Result<(), io::Error>
where
    C: Connect,
{
    let s = Arc::new(Mutex::new(State {
        last_user_facing_request: Instant::now(),
        last_background_upload: Instant::now(),
    }));

    process_existing_action_caches(config.clone());

    let (uploader, channel) = ::net::background_uploader::start_uploader(config, &http_client, &s);
    let terminator = ::net::terminator::start_terminator(config, &s);

    let cfg = config.clone();

    let new_service = move || {
        // Move a clone of `client` into the `service_fn`.
        let downloader = downloader.clone();
        let http_client = http_client.clone();
        let request_upload = channel.clone();
        let state = Arc::clone(&s);

        let inner_cfg = cfg.clone();
        service_fn(move |req| {
            {
                let mut locked = state.lock().unwrap();
                locked.last_user_facing_request = Instant::now();
            }
            Box::new(
                match req.method() {
                    &Method::GET => get_request(
                        Instant::now(),
                        req,
                        &downloader,
                        &http_client,
                        &inner_cfg.clone(),
                    ),
                    &Method::PUT => put_request(
                        Instant::now(),
                        req,
                        &downloader,
                        &inner_cfg.clone(),
                        &request_upload,
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

    let server_engine = match config.bind_target.scheme() {
        Some("unix") => {
            info!(
                "Going to remove existing socket path: {}",
                config.bind_target.authority().unwrap()
            );

            // remove the socket we wish to bind to before binding to it.
            fs::remove_file(config.bind_target.authority().unwrap())?;
            info!(
                "Going to bind/start server on unix socket for: {}",
                config.bind_target
            );
            start_unix_server_impl(&config.bind_target, new_service).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Error configuring unix server: {:?}", e),
                )
            })?
        }
        Some("http") => {
            info!(
                "Going to bind/start server on http path for: 127.0.0.1:{}",
                config.bind_target.port().unwrap()
            );
            start_http_server_impl(&config.bind_target, new_service).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Error configuring unix server: {:?}", e),
                )
            })?
        }
        _o => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "Invalid bind target {}, didn't understand the scheme",
                    config.bind_target
                ),
            ))
        }
    };

    hyper::rt::run(
        server_engine
            .join(uploader)
            .join(terminator)
            .map(|_| ())
            .map_err(|_| ()),
    );
    return Ok(());
}
