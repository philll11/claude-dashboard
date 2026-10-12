//! The data thread: it runs the dashboard's server in this process, answers the interface's
//! requests one at a time, and passes the server's notifications on (ADR 0009).

use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use claude_dashboard_server::Server;
use claude_dashboard_server::app::App;
use crossbeam_channel::{Receiver, Sender, unbounded};

use super::answers::Ask;
use super::state::Event;

pub struct Data {
    asks: Option<Sender<Ask>>,
    /// Set once the interface quits, so that requests still waiting go unanswered.
    quitting: Arc<AtomicBool>,
    /// Says when the server has stopped.
    stopped: Receiver<()>,
    handle: Option<JoinHandle<()>>,
}

impl Data {
    /// Starts the server on a thread of its own, so that the first frame draws at once and a slow
    /// answer never holds up the keys.
    pub fn start(app: App, events: Sender<Event>) -> Self {
        let (asks, asked) = unbounded::<Ask>();
        let (done, stopped) = unbounded::<()>();
        let quitting = Arc::new(AtomicBool::new(false));
        let quit = Arc::clone(&quitting);
        let handle = thread::spawn(move || {
            let failed = events.clone();
            let work = panic::catch_unwind(AssertUnwindSafe(|| serve(app, &events, &asked, &quit)));
            if let Err(cause) = work {
                let reason = cause
                    .downcast_ref::<&str>()
                    .map(|text| (*text).to_owned())
                    .or_else(|| cause.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "the server stopped".to_owned());
                let _ = failed.send(Event::Failed(reason));
            }
            let _ = done.send(());
        });
        Self {
            asks: Some(asks),
            quitting,
            stopped,
            handle: Some(handle),
        }
    }

    pub fn ask(&self, ask: Ask) {
        if let Some(asks) = &self.asks {
            let _ = asks.send(ask);
        }
    }

    /// Stops the server, waiting at most `patience` for it: a read in progress stops between
    /// transcripts, but a request to the usage endpoint can take longer, and the process need
    /// not wait for that.
    pub fn stop(mut self, patience: Duration) {
        self.quitting.store(true, Ordering::Relaxed);
        drop(self.asks.take());
        if self.stopped.recv_timeout(patience).is_ok()
            && let Some(handle) = self.handle.take()
        {
            let _ = handle.join();
        }
    }
}

fn serve(app: App, events: &Sender<Event>, asked: &Receiver<Ask>, quitting: &AtomicBool) {
    let notifications = events.clone();
    let mut server = Server::start(
        app,
        Arc::new(move |method, params| {
            let _ = notifications.send(Event::Notification {
                method: method.to_owned(),
                params,
            });
        }),
    );
    for ask in asked {
        if quitting.load(Ordering::Relaxed) {
            break;
        }
        let result = server
            .answer(ask.method, ask.params)
            .map_err(|fault| fault.message);
        if events.send(Event::Answer { id: ask.id, result }).is_err() {
            break;
        }
    }
    server.stop();
}
