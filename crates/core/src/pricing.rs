//! API-equivalent cost: what a request would cost at Anthropic's published API prices.
//!
//! Costs are calculated whenever they are needed, from the tokens a request used and the dated
//! price table in `pricing/prices.json` (ADR 0004), so correcting the table corrects every cost.
//! A model the table does not list is unpriced, never free.

use std::collections::HashSet;
use std::sync::LazyLock;

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use serde::Deserialize;

use crate::tokens::Tokens;

static BUNDLED: LazyLock<PriceTable> = LazyLock::new(|| {
    PriceTable::parse(include_str!("../../../pricing/prices.json"))
        .expect("a test checks that the bundled price table is valid")
});

/// The prices of every model on Anthropic's pricing page, as checked on one date.
#[derive(Debug, Clone)]
pub struct PriceTable {
    source: String,
    checked: Date,
    web_search_per_thousand: f64,
    us_only_inference_multiplier: f64,
    models: Vec<Model>,
}

#[derive(Debug, Clone)]
struct Model {
    name: String,
    ids: Vec<String>,
    /// Oldest first; each applies from its date until the next one's.
    periods: Vec<Period>,
}

#[derive(Debug, Clone)]
struct Period {
    from: Option<Date>,
    standard: Rates,
    fast: Option<Fast>,
    long_prompt: Option<LongPrompt>,
}

#[derive(Debug, Clone, Copy)]
struct Fast {
    input: f64,
    output: f64,
}

#[derive(Debug, Clone, Copy)]
struct LongPrompt {
    above: u64,
    rates: Rates,
}

/// Prices in US dollars per million tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rates {
    pub input: f64,
    pub cache_write_5m: f64,
    pub cache_write_1h: f64,
    pub cache_read: f64,
    pub output: f64,
}

impl Rates {
    fn scaled(self, factor: f64) -> Self {
        Self {
            input: self.input * factor,
            cache_write_5m: self.cache_write_5m * factor,
            cache_write_1h: self.cache_write_1h * factor,
            cache_read: self.cache_read * factor,
            output: self.output * factor,
        }
    }
}

/// What the price of a request depends on.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    /// The model id, as Claude Code records it.
    pub model: &'a str,
    /// When the request was made, in milliseconds since the Unix epoch.
    pub timestamp_ms: i64,
    pub tokens: Tokens,
    /// `fast` for fast mode.
    pub speed: Option<&'a str>,
    /// `us` for US-only inference.
    pub inference_geo: Option<&'a str>,
    pub web_search_requests: u64,
}

/// API-equivalent cost in US dollars, by what it pays for.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Cost {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write_5m: f64,
    pub cache_write_1h: f64,
    pub web_search: f64,
}

impl Cost {
    #[must_use]
    pub fn total(&self) -> f64 {
        self.input
            + self.output
            + self.cache_read
            + self.cache_write_5m
            + self.cache_write_1h
            + self.web_search
    }

    pub fn add(&mut self, other: Self) {
        self.input += other.input;
        self.output += other.output;
        self.cache_read += other.cache_read;
        self.cache_write_5m += other.cache_write_5m;
        self.cache_write_1h += other.cache_write_1h;
        self.web_search += other.web_search;
    }
}

/// Why a price table cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum PriceTableError {
    #[error("the price table cannot be read: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the price table is invalid: {0}")]
    Invalid(String),
}

impl PriceTable {
    /// The price table this version of claude-dashboard ships with.
    #[must_use]
    pub fn bundled() -> &'static Self {
        &BUNDLED
    }

    /// Reads a price table in the format of `pricing/prices.json`.
    ///
    /// # Errors
    ///
    /// [`PriceTableError`] when the table is not valid JSON in that format, or breaks one of its
    /// rules: every model has an id and prices from the start, ids are unique, later prices are
    /// in date order, and prices are not negative.
    pub fn parse(json: &str) -> Result<Self, PriceTableError> {
        let raw: RawTable = serde_json::from_str(json)?;
        let invalid = |message: String| Err(PriceTableError::Invalid(message));
        let mut seen = HashSet::new();
        let mut models = Vec::with_capacity(raw.models.len());
        for model in raw.models {
            if model.ids.is_empty() {
                return invalid(format!("{} has no model id", model.name));
            }
            for id in &model.ids {
                if !seen.insert(id.clone()) {
                    return invalid(format!("{id} appears more than once"));
                }
            }
            let mut periods = Vec::with_capacity(model.prices.len());
            for (index, period) in model.prices.into_iter().enumerate() {
                let period = period.validate(&model.name)?;
                match (
                    index,
                    period.from,
                    periods.last().and_then(|p: &Period| p.from),
                ) {
                    (0, Some(_), _) => {
                        return invalid(format!("{}'s first prices start on a date", model.name));
                    }
                    (1.., None, _) => {
                        return invalid(format!("{}'s later prices need a date", model.name));
                    }
                    (1.., Some(from), Some(previous)) if from <= previous => {
                        return invalid(format!("{}'s prices are not in date order", model.name));
                    }
                    _ => periods.push(period),
                }
            }
            if periods.is_empty() {
                return invalid(format!("{} has no prices", model.name));
            }
            models.push(Model {
                name: model.name,
                ids: model.ids,
                periods,
            });
        }
        for value in [
            raw.web_search_per_thousand,
            raw.us_only_inference_multiplier,
        ] {
            if !value.is_finite() || value < 0.0 {
                return invalid(format!("{value} is not a price"));
            }
        }
        Ok(Self {
            source: raw.source,
            checked: raw.checked,
            web_search_per_thousand: raw.web_search_per_thousand,
            us_only_inference_multiplier: raw.us_only_inference_multiplier,
            models,
        })
    }

    /// The page the prices come from.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// When the prices were last checked against their source.
    #[must_use]
    pub fn checked(&self) -> Date {
        self.checked
    }

    /// The name of the model behind a model id, such as `Claude Opus 5.5`; `None` for an unpriced
    /// model.
    #[must_use]
    pub fn model_name(&self, model_id: &str) -> Option<&str> {
        self.model(model_id).map(|model| model.name.as_str())
    }

    /// The prices a request pays per million tokens; `None` for an unpriced model.
    #[must_use]
    pub fn rates(&self, request: &Request<'_>) -> Option<Rates> {
        let period = self.period(request.model, request.timestamp_ms)?;

        let mut rates = match period.long_prompt {
            Some(long) if request.tokens.prompt() > long.above => long.rates,
            _ => period.standard,
        };
        if let (Some("fast"), Some(fast)) = (request.speed, period.fast) {
            // Prompt caching multiplies the fast input price the way it does the standard one.
            let standard = period.standard;
            rates = Rates {
                input: fast.input,
                cache_write_5m: fast.input * standard.cache_write_5m / standard.input,
                cache_write_1h: fast.input * standard.cache_write_1h / standard.input,
                cache_read: fast.input * standard.cache_read / standard.input,
                output: fast.output,
            };
        }
        if request.inference_geo == Some("us") {
            rates = rates.scaled(self.us_only_inference_multiplier);
        }
        Some(rates)
    }

    /// A request's API-equivalent cost; `None` for an unpriced model.
    #[must_use]
    pub fn cost(&self, request: &Request<'_>) -> Option<Cost> {
        let rates = self.rates(request)?;
        let tokens = request.tokens;
        Some(Cost {
            input: dollars(tokens.input, rates.input),
            output: dollars(tokens.output, rates.output),
            cache_read: dollars(tokens.cache_read, rates.cache_read),
            cache_write_5m: dollars(tokens.cache_write_5m, rates.cache_write_5m),
            cache_write_1h: dollars(tokens.cache_write_1h, rates.cache_write_1h),
            web_search: count(request.web_search_requests) * self.web_search_per_thousand / 1e3,
        })
    }

    /// A model's prices for prompts longer than a number of tokens, where it has such a tier: the
    /// number of tokens, and the prices above it.
    #[must_use]
    pub fn long_prompt(&self, model_id: &str, timestamp_ms: i64) -> Option<(u64, Rates)> {
        let long = self.period(model_id, timestamp_ms)?.long_prompt?;
        Some((long.above, long.rates))
    }

    /// A model's fast-mode prices of input and output, where it has a fast mode.
    #[must_use]
    pub fn fast(&self, model_id: &str, timestamp_ms: i64) -> Option<(f64, f64)> {
        let fast = self.period(model_id, timestamp_ms)?.fast?;
        Some((fast.input, fast.output))
    }

    /// The prices a model has at a moment.
    fn period(&self, model_id: &str, timestamp_ms: i64) -> Option<&Period> {
        let model = self.model(model_id)?;
        let day = Timestamp::from_millisecond(timestamp_ms)
            .ok()?
            .to_zoned(TimeZone::UTC)
            .date();
        model
            .periods
            .iter()
            .rev()
            .find(|period| period.from.is_none_or(|from| from <= day))
    }

    fn model(&self, model_id: &str) -> Option<&Model> {
        let id = base_id(model_id);
        self.models
            .iter()
            .find(|model| model.ids.iter().any(|known| known == id))
    }
}

fn dollars(tokens: u64, per_million: f64) -> f64 {
    count(tokens) * per_million / 1e6
}

#[expect(
    clippy::cast_precision_loss,
    reason = "counts stay far below 2^53, where f64 starts to skip whole numbers"
)]
fn count(value: u64) -> f64 {
    value as f64
}

/// The id a model goes by in the price table, without the decorations that dated snapshots,
/// Amazon Bedrock, Google Cloud and Claude Code add to it.
fn base_id(model_id: &str) -> &str {
    let mut id = model_id.trim();
    // Claude Code's context variants, such as `claude-opus-4-6[1m]`.
    if let Some((name, _)) = id.split_once('[') {
        id = name;
    }
    // Bedrock: `anthropic.claude-…`, optionally after a region such as `us.` or `global.`.
    if let Some(at) = id.find("anthropic.") {
        id = &id[at + "anthropic.".len()..];
    }
    id = id
        .strip_suffix("-v1:0")
        .or_else(|| id.strip_suffix("-v1"))
        .unwrap_or(id);
    // Google Cloud dates a snapshot with `@`, the Claude API with `-`.
    for separator in ['@', '-'] {
        if let Some((name, date)) = id.rsplit_once(separator)
            && date.len() == 8
            && date.bytes().all(|byte| byte.is_ascii_digit())
        {
            id = name;
            break;
        }
    }
    id.strip_suffix("-latest").unwrap_or(id)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTable {
    source: String,
    checked: Date,
    web_search_per_thousand: f64,
    us_only_inference_multiplier: f64,
    models: Vec<RawModel>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawModel {
    name: String,
    ids: Vec<String>,
    prices: Vec<RawPeriod>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPeriod {
    #[serde(default)]
    from: Option<Date>,
    input: f64,
    cache_write_5m: f64,
    cache_write_1h: f64,
    cache_read: f64,
    output: f64,
    #[serde(default)]
    fast: Option<RawFast>,
    #[serde(default)]
    long_prompt: Option<RawLongPrompt>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFast {
    input: f64,
    output: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLongPrompt {
    above: u64,
    input: f64,
    cache_write_5m: f64,
    cache_write_1h: f64,
    cache_read: f64,
    output: f64,
}

impl RawPeriod {
    fn validate(self, model: &str) -> Result<Period, PriceTableError> {
        let standard = rates(
            model,
            [
                self.input,
                self.cache_write_5m,
                self.cache_write_1h,
                self.cache_read,
                self.output,
            ],
        )?;
        if standard.input <= 0.0 {
            return Err(PriceTableError::Invalid(format!(
                "{model}'s input price must be above zero"
            )));
        }
        let fast = match self.fast {
            Some(fast) => {
                rates(model, [fast.input, fast.output, 0.0, 0.0, 0.0])?;
                Some(Fast {
                    input: fast.input,
                    output: fast.output,
                })
            }
            None => None,
        };
        let long_prompt = match self.long_prompt {
            Some(long) => Some(LongPrompt {
                above: long.above,
                rates: rates(
                    model,
                    [
                        long.input,
                        long.cache_write_5m,
                        long.cache_write_1h,
                        long.cache_read,
                        long.output,
                    ],
                )?,
            }),
            None => None,
        };
        if fast.is_some() && long_prompt.is_some() {
            return Err(PriceTableError::Invalid(format!(
                "{model} has both fast-mode and long-prompt prices, which the pricing page never combines"
            )));
        }
        Ok(Period {
            from: self.from,
            standard,
            fast,
            long_prompt,
        })
    }
}

fn rates(model: &str, prices: [f64; 5]) -> Result<Rates, PriceTableError> {
    if let Some(price) = prices
        .iter()
        .find(|price| !price.is_finite() || **price < 0.0)
    {
        return Err(PriceTableError::Invalid(format!(
            "{model} has a price of {price}"
        )));
    }
    let [input, five_minutes, one_hour, read, output] = prices;
    Ok(Rates {
        input,
        cache_write_5m: five_minutes,
        cache_write_1h: one_hour,
        cache_read: read,
        output,
    })
}

#[cfg(test)]
mod tests {
    use super::base_id;

    #[test]
    fn model_ids_lose_their_decorations() {
        for (recorded, base) in [
            ("claude-opus-5-5", "claude-opus-5-5"),
            ("claude-haiku-4-5-20251001", "claude-haiku-4-5"),
            ("claude-opus-4-20250514", "claude-opus-4"),
            ("claude-3-5-haiku-20241022", "claude-3-5-haiku"),
            ("claude-3-5-haiku-latest", "claude-3-5-haiku"),
            ("claude-haiku-4-5@20251001", "claude-haiku-4-5"),
            ("anthropic.claude-opus-5", "claude-opus-5"),
            (
                "us.anthropic.claude-sonnet-4-5-20250929-v1:0",
                "claude-sonnet-4-5",
            ),
            ("global.anthropic.claude-opus-4-6-v1", "claude-opus-4-6"),
            ("claude-opus-4-6[1m]", "claude-opus-4-6"),
            ("claude-sonnet-5", "claude-sonnet-5"),
        ] {
            assert_eq!(base_id(recorded), base, "{recorded}");
        }
    }
}
