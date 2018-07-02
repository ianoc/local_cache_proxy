use futures;
use hyper;
use std;
use std::error::Error as StdError;
use std::fmt;

#[derive(Debug)]
pub enum ServerError {
    IoError(std::io::Error),
    HyperError(::hyper::Error),
    StreamingError(futures::sync::mpsc::SendError<Result<hyper::Chunk, std::io::Error>>),
    StringError(String),
    HttpError(::http::Error),
    InvalidUri(::http::uri::InvalidUri),
    InvalidUriParts(::http::uri::InvalidUriParts),
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.description())
    }
}

impl StdError for ServerError {
    fn description(&self) -> &str {
        match self {
            ServerError::IoError(e) => {
                error!("errr:: {:?}", e);
                StdError::description(e)
            }
            ServerError::HyperError(e) => StdError::description(e),
            ServerError::StreamingError(e) => StdError::description(e),
            ServerError::StringError(e) => e,
            ServerError::HttpError(e) => StdError::description(e),
            ServerError::InvalidUri(e) => StdError::description(e),
            ServerError::InvalidUriParts(e) => StdError::description(e),
        }
    }
}
impl From<std::io::Error> for ServerError {
    fn from(error: std::io::Error) -> Self {
        ServerError::IoError(error)
    }
}

impl From<::hyper::Error> for ServerError {
    fn from(error: ::hyper::Error) -> Self {
        ServerError::HyperError(error)
    }
}

impl From<String> for ServerError {
    fn from(error: String) -> Self {
        ServerError::StringError(error)
    }
}

impl From<::http::Error> for ServerError {
    fn from(error: ::http::Error) -> Self {
        ServerError::HttpError(error)
    }
}

impl From<::http::uri::InvalidUri> for ServerError {
    fn from(error: ::http::uri::InvalidUri) -> Self {
        ServerError::InvalidUri(error)
    }
}

impl From<::http::uri::InvalidUriParts> for ServerError {
    fn from(error: ::http::uri::InvalidUriParts) -> Self {
        ServerError::InvalidUriParts(error)
    }
}

impl std::convert::From<futures::sync::mpsc::SendError<Result<hyper::Chunk, std::io::Error>>>
    for ServerError
{
    fn from(error: futures::sync::mpsc::SendError<Result<hyper::Chunk, std::io::Error>>) -> Self {
        ServerError::StreamingError(error)
    }
}
