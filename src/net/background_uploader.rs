use hyper::client::connect::Connect;
use hyper::Client;
use http::Uri;
use http::Response;
use net::buffered_send_stream;

use http::Request;

use hyper::{Body, StatusCode};

use futures::Future;
use futures;
use hyper::rt;
use futures::future::Either;

pub fn raw_upload_file<C: Connect + 'static>(
    http_client: Client<C>,
    uri: Uri,
    path: String,
) -> Box<Future<Item = Response<Body>, Error = String> + Send + 'static> {

    info!("Uploading {} to {:?}", path, uri);
    let body = match buffered_send_stream::send_file(&path) {
        Ok(body) => body,
        Err(e) => {
            return Box::new(futures::future::err(e).map_err(
                |e: ::std::io::Error| e.to_string(),
            ))
        }
    };


    let http_payload = http_client.request(Request::put(uri.clone()).body(body).unwrap());


    Box::new(http_payload.map_err(|e| e.to_string()))
}


pub fn spawn_upload_file<C: Connect + 'static>(
    http_client: Client<C>,
    uri: Uri,
    path: String,
) -> () {
    let u = uri.clone();
    rt::spawn(
        raw_upload_file(http_client, uri, path)
            .map_err(|e| {
                warn!("Error in upload: {:?}", e);
            })
            .map(move |_e| info!("Upload complete! to {:?}", u)),
    );
}

pub fn maybe_upload_file<C: Connect + 'static>(
    http_client: Client<C>,
    uri: Uri,
    path: String,
) -> () {
    info!("Maybe uploading {} to {:?}", path, uri);
    let fut = ::net::client::connect_for_file(http_client.clone(), uri.clone(), 3)
        .map_err(|e| {
            warn!("Error in check if file exists: {:?}", e);
        })
        .and_then(|resp| if resp.status() == StatusCode::OK {
            Either::A(futures::future::ok(()))
        } else {
            Either::B(
                raw_upload_file(http_client, uri, path)
                    .map_err(|e| {
                        warn!("Error in upload: {:?}", e);
                    })
                    .map(|_e| ()),
            )
        });

    rt::spawn(
        fut.map_err(|e| {
            warn!("Error in upload: {:?}", e);
        }).map(|_e| ()),
    );




}
