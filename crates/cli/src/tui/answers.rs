//! The server's answers, kept by request and asked for again once the history or the plan limits
//! change, so that what the screen shows updates in place instead of blanking.

use std::collections::HashMap;

use claude_dashboard_server::protocol::{
    Choices, Health, ImportOffer, Limits, Overview, Prices, SessionDetail, SessionsPage, Status,
};
use serde::de::DeserializeOwned;
use serde_json::Value;

/// A request the screen needs answered.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub method: &'static str,
    pub params: Value,
}

impl Request {
    pub fn new(method: &'static str, params: Value) -> Self {
        Self { method, params }
    }

    fn key(&self) -> String {
        format!("{} {}", self.method, self.params)
    }
}

/// A request on its way to the server, by its number.
#[derive(Debug, Clone, PartialEq)]
pub struct Ask {
    pub id: u64,
    pub method: &'static str,
    pub params: Value,
}

/// An answer, read into its type.
#[derive(Debug, Clone)]
pub enum Answer {
    Status(Box<Status>),
    Overview(Box<Overview>),
    Sessions(Box<SessionsPage>),
    Session(Box<SessionDetail>),
    Limits(Box<Limits>),
    Prices(Box<Prices>),
    Choices(Box<Choices>),
    Health(Box<Health>),
    ImportOffer(Option<Box<ImportOffer>>),
}

fn read<T: DeserializeOwned>(value: Value) -> Result<Box<T>, String> {
    serde_json::from_value(value)
        .map(Box::new)
        .map_err(|error| error.to_string())
}

impl Answer {
    fn read(method: &str, value: Value) -> Result<Self, String> {
        Ok(match method {
            "status" => Self::Status(read(value)?),
            "overview" => Self::Overview(read(value)?),
            "sessions" => Self::Sessions(read(value)?),
            "session" => Self::Session(read(value)?),
            "limits" => Self::Limits(read(value)?),
            "prices" => Self::Prices(read(value)?),
            "choices" => Self::Choices(read(value)?),
            "health" => Self::Health(read(value)?),
            "import/offer" => Self::ImportOffer(if value.is_null() {
                None
            } else {
                Some(read(value)?)
            }),
            other => return Err(format!("no answer is kept for {other}")),
        })
    }
}

#[derive(Debug)]
struct Entry {
    request: Request,
    answer: Option<Answer>,
    error: Option<String>,
    /// Set while the answer is out of date.
    stale: bool,
    /// The request on its way, if one is.
    asking: Option<u64>,
    /// When the screen last needed it, in rounds of asking.
    needed: u64,
}

/// How many answers that the screen no longer needs are kept, for going back to them.
const KEEP: usize = 40;

#[derive(Debug, Default)]
pub struct Answers {
    entries: HashMap<String, Entry>,
    next_id: u64,
    round: u64,
}

impl Answers {
    pub fn get(&self, request: &Request) -> Option<&Answer> {
        self.entries.get(&request.key())?.answer.as_ref()
    }

    /// Why the server could not answer `request`, while it has no answer to show.
    pub fn error(&self, request: &Request) -> Option<&str> {
        let entry = self.entries.get(&request.key())?;
        entry.answer.is_none().then_some(entry.error.as_deref()?)
    }

    /// A request whose answer nobody keeps, such as reading new transcripts now.
    pub fn command(&mut self, method: &'static str, params: Value) -> Ask {
        self.next_id += 1;
        Ask {
            id: self.next_id,
            method,
            params,
        }
    }

    /// The requests among `needed` to send now: those never asked and those whose answer is out
    /// of date, unless one is on its way already.
    pub fn due(&mut self, needed: &[Request]) -> Vec<Ask> {
        self.round += 1;
        let mut asks = Vec::new();
        for request in needed {
            let entry = self.entries.entry(request.key()).or_insert_with(|| Entry {
                request: request.clone(),
                answer: None,
                error: None,
                stale: true,
                asking: None,
                needed: 0,
            });
            entry.needed = self.round;
            if entry.stale && entry.asking.is_none() {
                self.next_id += 1;
                entry.stale = false;
                entry.asking = Some(self.next_id);
                asks.push(Ask {
                    id: self.next_id,
                    method: entry.request.method,
                    params: entry.request.params.clone(),
                });
            }
        }
        self.forget();
        asks
    }

    /// Keeps the answer to request `id`.
    pub fn answered(&mut self, id: u64, result: Result<Value, String>) {
        let Some(entry) = self
            .entries
            .values_mut()
            .find(|entry| entry.asking == Some(id))
        else {
            return;
        };
        entry.asking = None;
        match result.and_then(|value| Answer::read(entry.request.method, value)) {
            Ok(answer) => {
                entry.answer = Some(answer);
                entry.error = None;
            }
            Err(error) => entry.error = Some(error),
        }
    }

    /// Marks the answers to the methods `which` picks as out of date, so that they are asked
    /// again while the screen needs them.
    pub fn outdate(&mut self, which: impl Fn(&str) -> bool) {
        for entry in self.entries.values_mut() {
            if which(entry.request.method) {
                entry.stale = true;
            }
        }
    }

    /// Drops the answers needed longest ago, beyond the few kept for going back.
    fn forget(&mut self) {
        let mut unneeded: Vec<(u64, String)> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.needed < self.round && entry.asking.is_none())
            .map(|(key, entry)| (entry.needed, key.clone()))
            .collect();
        if unneeded.len() <= KEEP {
            return;
        }
        unneeded.sort();
        for (_, key) in unneeded.iter().take(unneeded.len() - KEEP) {
            self.entries.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn status() -> Request {
        Request::new("status", Value::Null)
    }

    #[test]
    fn a_request_is_asked_once_until_it_is_out_of_date() {
        let mut answers = Answers::default();
        let asks = answers.due(&[status()]);
        assert_eq!(asks.len(), 1);
        assert_eq!(answers.due(&[status()]), [], "on its way");

        answers.answered(asks[0].id, Err("not yet".to_owned()));
        assert_eq!(answers.error(&status()), Some("not yet"));
        assert_eq!(answers.due(&[status()]), [], "answered");

        answers.outdate(|method| method == "status");
        assert_eq!(answers.due(&[status()]).len(), 1);
    }

    #[test]
    fn a_change_while_a_request_is_on_its_way_asks_again_after_its_answer() {
        let mut answers = Answers::default();
        let request = Request::new("import/offer", Value::Null);
        let first = answers.due(std::slice::from_ref(&request));
        answers.outdate(|_| true);
        assert_eq!(answers.due(std::slice::from_ref(&request)), []);

        answers.answered(first[0].id, Ok(json!(null)));
        assert!(matches!(
            answers.get(&request),
            Some(Answer::ImportOffer(None))
        ));
        assert_eq!(answers.due(std::slice::from_ref(&request)).len(), 1);
    }

    #[test]
    fn answers_no_longer_needed_are_dropped_beyond_a_few() {
        let mut answers = Answers::default();
        for n in 0..(KEEP + 10) {
            let request = Request::new("session", json!({ "id": n.to_string() }));
            let asks = answers.due(std::slice::from_ref(&request));
            answers.answered(asks[0].id, Err("gone".to_owned()));
        }
        answers.due(&[]);
        assert_eq!(answers.entries.len(), KEEP);
        assert!(
            answers
                .error(&Request::new("session", json!({ "id": "0" })))
                .is_none()
        );
    }
}
