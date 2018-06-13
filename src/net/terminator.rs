use config::AppConfig;
use futures::stream::Stream;
use futures::Future;
use futures::Poll;
use net::server::State;
use std::process;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;
use tokio::prelude::*;
use tokio::timer::Delay;

pub(super) fn start_terminator(
    config: &AppConfig,
    state: &Arc<Mutex<State>>,
) -> Box<Future<Item = (), Error = ()> + Send> {
    Box::new(Terminator {
        active_future: None,
        config: config.clone(),
        state: Arc::clone(state),
    })
}

struct Terminator {
    /// this is the active worker future that might be complete
    active_future: Option<Box<Future<Item = (), Error = String> + Send + 'static>>,

    /// Our application config
    config: AppConfig,

    /// Shared states between clients and services
    state: Arc<Mutex<State>>,
}

impl Future for Terminator {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        // Receive all messages from peers.
        match &mut self.active_future {
            Some(fut) => match fut.poll() {
                Ok(Async::Ready(_)) => (),
                Err(e) => {
                    warn!("Failed to poll active future with {:?}", e);
                }
                Ok(Async::NotReady) => return Ok(Async::NotReady),
            },
            None => (),
        };
        self.active_future = None;

        let time_since_last_req = {
            let s = self.state.lock().unwrap();
            s.0.elapsed()
        };
        // 30 mins idle time and die
        if time_since_last_req < Duration::from_millis(1000 * 60 * 30) {
            self.active_future = Some(Box::new(
                Delay::new(Instant::now() + Duration::from_millis(1000 * 60))
                    .map_err(|e| e.to_string()),
            ));
            // we need to self-poll here to ensure we get notified for changes in
            // the future behavior
            return self.poll();
        }
        process::exit(0);
    }
}
