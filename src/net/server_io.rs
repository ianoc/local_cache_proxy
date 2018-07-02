use futures::{future, Future};
use http::header;
use http::header::HeaderValue;
use http::Response;
use http::StatusCode;
use hyper::Body;
use net::buffered_send_stream;
use net::server_error::ServerError;
use std::fs;
use std::io::ErrorKind as IoErrorKind;

// Generate a response future for a given status code
pub fn empty_with_status_code_fut(
    status_code: StatusCode,
) -> impl Future<Item = Response<Body>, Error = ServerError> {
    future::ok(empty_with_status_code(status_code))
}

// Generate an empty body with a given status code
pub fn empty_with_status_code(status_code: StatusCode) -> Response<Body> {
    // We decided not to download the file, so 404 it!
    let mut res = Response::new(Body::empty());
    *res.status_mut() = status_code;
    res
}

type ResponseFuture = Box<Future<Item = Response<Body>, Error = ServerError> + Send>;

pub fn send_file(path: String) -> ResponseFuture {
    let metadata = match fs::metadata(&path) {
        Ok(meta) => meta,
        Err(e) => {
            return match e.kind() {
                IoErrorKind::NotFound => panic!(
                    "Should never reach here, file not found looking for {:?}",
                    path
                ),
                IoErrorKind::PermissionDenied => {
                    Box::new(empty_with_status_code_fut(StatusCode::FORBIDDEN).map_err(From::from))
                }
                _ => Box::new(future::err(e).map_err(From::from)),
            };
        }
    };
    // Build response headers.
    let size: String = metadata.len().to_string();
    let mut res = Response::builder();

    let header_size = match HeaderValue::from_str(&size) {
        Err(_e) => {
            return Box::new(
                future::err("Unable to extract size from file system".to_string())
                    .map_err(From::from),
            )
        }
        Ok(s) => s,
    };

    res.header(header::CONTENT_LENGTH, header_size);

    let body = match buffered_send_stream::send_file(&path) {
        Ok(body) => body,
        Err(err) => return Box::new(future::err(err)),
    };
    Box::new(future::result(res.body(body)).map_err(From::from))
}
