use futures::Poll;
use futures::Stream;
use hyper::Body;
use hyper::Chunk;
use net::server::ServerError;
use std::io::Error;

use futures::sync::mpsc::SendError;
use std::fs::File;

use std::mem;

use futures::Async;
use futures::Future;
use hyper;
use std::io::Read;

pub fn send_file<E>(path: &String) -> Result<Body, E>
where
    E: From<Error>,
{
    let file = File::open(path).map_err(From::from)?;

    let (sender, body) = Body::channel();
    hyper::rt::spawn(
        BufferedSendStream::new(&path, FileChunkStream(file), sender)
            .map(|_| ())
            .map_err(|_| ()),
    );
    Ok(body)
}

struct FileChunkStream(pub File);
impl Stream for FileChunkStream {
    type Item = Result<Chunk, Error>;
    type Error = SendError<Self::Item>;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        // TODO: non-blocking read
        let mut buf: [u8; 4096] = unsafe { mem::uninitialized() };
        match self.0.read(&mut buf) {
            Ok(0) => Ok(Async::Ready(None)),
            Ok(size) => Ok(Async::Ready(Some(Ok(Chunk::from(buf[0..size].to_owned()))))),
            Err(err) => Ok(Async::Ready(Some(Err(err)))),
        }
    }
}

pub struct BufferedSendStream {
    file_name: String,
    file_chunk_stream: FileChunkStream,
    sender: hyper::body::Sender,
}
impl BufferedSendStream {
    fn new(
        file_name: &String,
        file_chunk_stream: FileChunkStream,
        sender: hyper::body::Sender,
    ) -> BufferedSendStream {
        BufferedSendStream {
            file_name: file_name.clone(),
            file_chunk_stream: file_chunk_stream,
            sender: sender,
        }
    }
}

impl Future for BufferedSendStream {
    type Item = ();
    type Error = ServerError;

    fn poll(&mut self) -> Result<Async<()>, ServerError> {
        loop {
            match self.sender.poll_ready() {
                Ok(Async::Ready(_)) => (),
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Err(_e) => return Ok(Async::Ready(())),
            };

            match self.file_chunk_stream.poll()? {
                Async::Ready(None) => return Ok(Async::Ready(())),
                Async::Ready(Some(Ok(buf))) => {
                    self.sender.send_data(buf).map_err(|_e| {
                        error!("Failed to send chunk for file {}", self.file_name);
                        "Failed to send chunk".to_string()
                    })?;
                    return self.poll();
                }
                Async::Ready(Some(Err(e))) => {
                    warn!("Failed to send file: {}, error: {:?}", self.file_name, e);
                    return Ok(Async::Ready(()));
                }
                Async::NotReady => panic!("File reading locally should never be not ready, its setup to be blocking currently"),
            }
        }
    }
}
