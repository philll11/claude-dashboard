//! The dashboard's server: the answers to its requests, and the background work that reads new
//! transcript content and tells the dashboard when the history or the plan limits change. Every
//! graphical surface runs it: `claude-dashboard serve --stdio` for VS Code (ADR 0006), and the
//! desktop app in its own process (ADR 0008). It knows no transport; each surface carries the
//! requests, answers and notifications its own way.

pub mod app;
pub mod fmt;
pub mod limits;
pub mod names;
pub mod program;
pub mod protocol;
mod requests;
pub mod statusline;
mod workers;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde_json::Value;

use crate::app::App;
pub use crate::requests::Fault;

/// Called for every notification the server sends, with its method and parameters.
pub type Notify = Arc<dyn Fn(&str, Value) + Send + Sync>;

/// What the background work knows that answers need.
#[derive(Debug, Default)]
struct Shared {
    /// Set while new transcript content is read.
    reading: Option<protocol::Progress>,
    /// When reading new transcript content last finished.
    read_ms: Option<i64>,
    /// What could not be read the last time, with the reason.
    problems: Vec<String>,
}

/// The shared state, whichever thread last held it; none of them leaves it half-changed.
fn lock(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The server: it answers requests one at a time, while its workers read new transcript content
/// and watch the history in the background.
pub struct Server {
    answers: requests::Answers,
    workers: workers::Workers,
}

impl Server {
    /// Starts the background work, which reports through `notify`.
    pub fn start(app: App, notify: Notify) -> Self {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let workers = workers::start(&app, notify, &shared);
        let answers = requests::Answers::new(app, shared, workers.triggers());
        Self { answers, workers }
    }

    /// The answer to one request: its result, or why there is none.
    ///
    /// # Errors
    ///
    /// The fault, with a JSON-RPC error code, when the method is unknown, its parameters do not
    /// fit, or the history cannot answer.
    pub fn answer(&mut self, method: &str, params: Value) -> Result<Value, Fault> {
        self.answers.answer(method, params)
    }

    /// Stops the background work and waits until it has.
    pub fn stop(self) {
        self.workers.stop();
    }
}
