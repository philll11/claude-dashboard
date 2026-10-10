//! What the history takes from one transcript record.

use std::fmt;
use std::marker::PhantomData;

use serde::Deserialize;
use serde::de::value::MapAccessDeserializer;
use serde::de::{self, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};

/// A transcript record that the history uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Record {
    Request(Box<RequestRecord>),
    /// The title Claude Code gave the session; the last one counts.
    AiTitle(String),
    /// The title the person gave the session; the last one counts, ahead of any AI title.
    CustomTitle(String),
}

/// A request, as one transcript record holds it.
///
/// Claude Code writes several records for one streamed response, each with the usage so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestRecord {
    /// The record's own id.
    pub record_id: String,
    pub message_id: Option<String>,
    pub request_id: Option<String>,
    /// When the record was written, in milliseconds since the Unix epoch.
    pub timestamp_ms: i64,
    /// The working folder.
    pub cwd: Option<String>,
    /// How Claude Code was started, such as `cli`, `sdk-cli` or `claude-vscode`.
    pub entrypoint: Option<String>,
    pub claude_code_version: Option<String>,
    pub speed: Option<String>,
    pub service_tier: Option<String>,
    pub inference_geo: Option<String>,
    pub skill: Option<String>,
    pub mcp_server: Option<String>,
    pub mcp_tool: Option<String>,
    pub web_search_requests: u64,
    pub web_fetch_requests: u64,
    /// The tokens used, by model: the record's own model first, then any other model the
    /// response used, such as one that Claude Code tried before falling back.
    pub usage: Vec<ModelUsage>,
}

/// The tokens one model used for a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelUsage {
    pub model: String,
    pub usage: Usage,
}

/// Tokens by type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
}

impl Usage {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn add(&mut self, other: Self) {
        self.input = self.input.saturating_add(other.input);
        self.output = self.output.saturating_add(other.output);
        self.cache_read = self.cache_read.saturating_add(other.cache_read);
        self.cache_write_5m = self.cache_write_5m.saturating_add(other.cache_write_5m);
        self.cache_write_1h = self.cache_write_1h.saturating_add(other.cache_write_1h);
    }
}

/// Why a line cannot be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unreadable {
    /// The line is not a JSON object.
    NotJson,
    /// A request without the record id, time or model that the history needs.
    Incomplete,
}

/// The model name Claude Code gives messages it writes itself, such as API errors.
const SYNTHETIC_MODEL: &str = "<synthetic>";

/// Reads one transcript line, with or without its line ending.
///
/// Returns `None` for a record the history does not use.
///
/// # Errors
///
/// [`Unreadable`] for a line that is not JSON, or a request that lacks what identifies it.
pub fn parse_line(line: &[u8]) -> Result<Option<Record>, Unreadable> {
    let line = line.trim_ascii();
    if line.is_empty() {
        return Ok(None);
    }
    // serde also reads a struct from a JSON array, which is no record.
    if line.first() != Some(&b'{') {
        return Err(Unreadable::NotJson);
    }
    let raw: RawRecord = serde_json::from_slice(line).map_err(|_| Unreadable::NotJson)?;
    match raw.kind.as_deref() {
        Some("assistant") => {
            request(raw).map(|request| request.map(|r| Record::Request(Box::new(r))))
        }
        Some("ai-title") => Ok(raw.ai_title.map(Record::AiTitle)),
        Some("custom-title") => Ok(raw.custom_title.map(Record::CustomTitle)),
        _ => Ok(None),
    }
}

fn request(raw: RawRecord) -> Result<Option<RequestRecord>, Unreadable> {
    let Some(message) = raw.message else {
        return Ok(None);
    };
    let Some(usage) = message.usage else {
        return Ok(None);
    };
    let model = message.model.ok_or(Unreadable::Incomplete)?;
    if model == SYNTHETIC_MODEL {
        return Ok(None);
    }
    let record_id = raw.uuid.ok_or(Unreadable::Incomplete)?;
    let timestamp_ms = raw
        .timestamp
        .and_then(|timestamp| timestamp.parse::<jiff::Timestamp>().ok())
        .ok_or(Unreadable::Incomplete)?
        .as_millisecond();
    let by_model = usage_by_model(&model, &usage);
    let server_tools = usage.server_tool_use.unwrap_or_default();
    Ok(Some(RequestRecord {
        usage: by_model,
        record_id,
        message_id: message.id,
        request_id: raw.request_id,
        timestamp_ms,
        cwd: raw.cwd,
        entrypoint: raw.entrypoint,
        claude_code_version: raw.version,
        speed: usage.speed,
        service_tier: usage.service_tier,
        inference_geo: usage.inference_geo,
        skill: raw.attribution_skill,
        mcp_server: raw.attribution_mcp_server,
        mcp_tool: raw.attribution_mcp_tool,
        web_search_requests: server_tools.web_search_requests.unwrap_or(0),
        web_fetch_requests: server_tools.web_fetch_requests.unwrap_or(0),
    }))
}

/// The tokens a response used, by model.
///
/// The iterations list each call behind the response with the model it went to; the top-level
/// usage repeats the last call, and is empty when the response was cut off mid-stream. Without
/// iterations, the top-level usage is all there is.
fn usage_by_model(model: &str, usage: &RawUsage) -> Vec<ModelUsage> {
    let mut parts: Vec<ModelUsage> = Vec::new();
    for iteration in &usage.iterations {
        let iteration_model = iteration.model.as_deref().unwrap_or(model);
        let tokens = iteration.tokens.usage();
        match parts.iter_mut().find(|part| part.model == iteration_model) {
            Some(part) => part.usage.add(tokens),
            None => parts.push(ModelUsage {
                model: iteration_model.to_owned(),
                usage: tokens,
            }),
        }
    }
    if parts.iter().all(|part| part.usage.is_empty()) {
        return vec![ModelUsage {
            model: model.to_owned(),
            usage: usage.tokens.usage(),
        }];
    }
    if let Some(own) = parts.iter().position(|part| part.model == model) {
        parts[..=own].rotate_right(1);
    }
    parts
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRecord {
    #[serde(rename = "type", default, deserialize_with = "lenient")]
    kind: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    uuid: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    timestamp: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    request_id: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    cwd: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    entrypoint: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    version: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    attribution_skill: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    attribution_mcp_server: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    attribution_mcp_tool: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    ai_title: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    custom_title: Option<String>,
    #[serde(default, deserialize_with = "object")]
    message: Option<RawMessage>,
}

#[derive(Deserialize)]
struct RawMessage {
    #[serde(default, deserialize_with = "lenient")]
    id: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    model: Option<String>,
    #[serde(default, deserialize_with = "object")]
    usage: Option<RawUsage>,
}

#[derive(Deserialize)]
struct RawUsage {
    #[serde(flatten)]
    tokens: RawTokens,
    #[serde(default, deserialize_with = "object")]
    server_tool_use: Option<RawServerTools>,
    #[serde(default, deserialize_with = "lenient")]
    speed: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    service_tier: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    inference_geo: Option<String>,
    #[serde(default, deserialize_with = "lenient_list")]
    iterations: Vec<RawIteration>,
}

#[derive(Deserialize)]
struct RawIteration {
    #[serde(flatten)]
    tokens: RawTokens,
    #[serde(default, deserialize_with = "lenient")]
    model: Option<String>,
}

#[derive(Deserialize)]
struct RawTokens {
    #[serde(default, deserialize_with = "lenient")]
    input_tokens: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    output_tokens: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    cache_read_input_tokens: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    cache_creation_input_tokens: Option<u64>,
    #[serde(default, deserialize_with = "object")]
    cache_creation: Option<RawCacheCreation>,
}

impl RawTokens {
    fn usage(&self) -> Usage {
        let split = self.cache_creation.as_ref();
        let five_minutes = split.and_then(|s| s.ephemeral_5m_input_tokens).unwrap_or(0);
        let one_hour = split.and_then(|s| s.ephemeral_1h_input_tokens).unwrap_or(0);
        let written = self
            .cache_creation_input_tokens
            .unwrap_or(five_minutes.saturating_add(one_hour));
        // Before Claude Code reported how long cache writes are kept, every write was kept for
        // five minutes, so whatever the split does not put at one hour counts as five minutes.
        let one_hour = one_hour.min(written);
        Usage {
            input: self.input_tokens.unwrap_or(0),
            output: self.output_tokens.unwrap_or(0),
            cache_read: self.cache_read_input_tokens.unwrap_or(0),
            cache_write_5m: written - one_hour,
            cache_write_1h: one_hour,
        }
    }
}

#[derive(Deserialize)]
struct RawCacheCreation {
    #[serde(default, deserialize_with = "lenient")]
    ephemeral_5m_input_tokens: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    ephemeral_1h_input_tokens: Option<u64>,
}

#[derive(Deserialize, Default)]
struct RawServerTools {
    #[serde(default, deserialize_with = "lenient")]
    web_search_requests: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    web_fetch_requests: Option<u64>,
}

/// Reads a small field as `T`, or as `None` when it holds another type, so that one changed field
/// does not cost the whole record.
fn lenient<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Field<T> {
        Value(T),
        Other(IgnoredAny),
    }
    Ok(match Field::<T>::deserialize(deserializer)? {
        Field::Value(value) => Some(value),
        Field::Other(_) => None,
    })
}

/// Reads a list, keeping the elements that read as `T`; anything other than a list reads as empty.
fn lenient_list<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(lenient::<D, Vec<Lenient<T>>>(deserializer)?
        .unwrap_or_default()
        .into_iter()
        .filter_map(|element| element.0)
        .collect())
}

/// One element of a list read by [`lenient_list`].
struct Lenient<T>(Option<T>);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Lenient<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        lenient(deserializer).map(Lenient)
    }
}

/// Reads an object as `T`, or as `None` when the field holds something else.
///
/// Unlike [`lenient`], it reads the object in place instead of buffering it first, because a
/// record's message holds the whole response.
fn object<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct ObjectVisitor<T>(PhantomData<T>);

    impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
        type Value = Option<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("any value")
        }

        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
            T::deserialize(MapAccessDeserializer::new(map)).map(Some)
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            while seq.next_element::<IgnoredAny>()?.is_some() {}
            Ok(None)
        }

        fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
    }

    deserializer.deserialize_any(ObjectVisitor(PhantomData))
}
