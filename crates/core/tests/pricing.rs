use claude_dashboard_core::pricing::{PriceTable, PriceTableError, Rates, Request};
use claude_dashboard_core::tokens::Tokens;

/// 2026-10-10T06:16:27.123Z.
const OCTOBER_2026: i64 = 1_791_612_987_123;

/// The model pricing table on <https://platform.claude.com/docs/en/about-claude/pricing> as of
/// 10 October 2026, copied separately from `pricing/prices.json` so that a slip in either shows:
/// the model id, its name, then US dollars per million tokens for input, five-minute cache
/// writes, one-hour cache writes, cache hits and output.
const PAGE: [(&str, &str, [f64; 5]); 20] = [
    (
        "claude-fable-5-1",
        "Claude Fable 5.1",
        [10.0, 12.5, 20.0, 0.25, 50.0],
    ),
    (
        "claude-mythos-5-1",
        "Claude Mythos 5.1",
        [10.0, 12.5, 20.0, 0.25, 50.0],
    ),
    (
        "claude-fable-5",
        "Claude Fable 5",
        [10.0, 12.5, 20.0, 1.0, 50.0],
    ),
    (
        "claude-mythos-5",
        "Claude Mythos 5",
        [10.0, 12.5, 20.0, 1.0, 50.0],
    ),
    (
        "claude-opus-5-5",
        "Claude Opus 5.5",
        [4.0, 5.0, 8.0, 0.2, 20.0],
    ),
    (
        "claude-opus-5",
        "Claude Opus 5",
        [5.0, 6.25, 10.0, 0.5, 25.0],
    ),
    (
        "claude-opus-4-8",
        "Claude Opus 4.8",
        [5.0, 6.25, 10.0, 0.5, 25.0],
    ),
    (
        "claude-opus-4-7",
        "Claude Opus 4.7",
        [5.0, 6.25, 10.0, 0.5, 25.0],
    ),
    (
        "claude-opus-4-6",
        "Claude Opus 4.6",
        [5.0, 6.25, 10.0, 0.5, 25.0],
    ),
    (
        "claude-opus-4-5",
        "Claude Opus 4.5",
        [5.0, 6.25, 10.0, 0.5, 25.0],
    ),
    (
        "claude-opus-4-1",
        "Claude Opus 4.1",
        [15.0, 18.75, 30.0, 1.5, 75.0],
    ),
    (
        "claude-opus-4",
        "Claude Opus 4",
        [15.0, 18.75, 30.0, 1.5, 75.0],
    ),
    (
        "claude-sonnet-5-5",
        "Claude Sonnet 5.5",
        [2.0, 2.5, 4.0, 0.1, 10.0],
    ),
    (
        "claude-sonnet-5",
        "Claude Sonnet 5",
        [2.0, 2.5, 4.0, 0.2, 10.0],
    ),
    (
        "claude-sonnet-4-6",
        "Claude Sonnet 4.6",
        [3.0, 3.75, 6.0, 0.3, 15.0],
    ),
    (
        "claude-sonnet-4-5",
        "Claude Sonnet 4.5",
        [3.0, 3.75, 6.0, 0.3, 15.0],
    ),
    (
        "claude-sonnet-4",
        "Claude Sonnet 4",
        [3.0, 3.75, 6.0, 0.3, 15.0],
    ),
    (
        "claude-haiku-5-5",
        "Claude Haiku 5.5",
        [0.1, 0.125, 0.2, 0.01, 0.5],
    ),
    (
        "claude-haiku-4-5",
        "Claude Haiku 4.5",
        [1.0, 1.25, 2.0, 0.1, 5.0],
    ),
    (
        "claude-3-5-haiku",
        "Claude Haiku 3.5",
        [0.8, 1.0, 1.6, 0.08, 4.0],
    ),
];

/// The page's prices for Claude Haiku 5.5 prompts over 100,000 tokens.
const HAIKU_5_5_LONG_PROMPT: [f64; 5] = [0.5, 0.625, 1.0, 0.05, 2.5];

fn each(count: u64) -> Tokens {
    Tokens {
        input: count,
        output: count,
        cache_read: count,
        cache_write_5m: count,
        cache_write_1h: count,
    }
}

fn request(model: &str, tokens: Tokens) -> Request<'_> {
    Request {
        model,
        timestamp_ms: OCTOBER_2026,
        tokens,
        speed: None,
        inference_geo: None,
        web_search_requests: 0,
    }
}

fn rates(model: &str, tokens: Tokens) -> [f64; 5] {
    let Rates {
        input,
        cache_write_5m,
        cache_write_1h,
        cache_read,
        output,
    } = PriceTable::bundled()
        .rates(&request(model, tokens))
        .unwrap_or_else(|| panic!("{model} is unpriced"));
    [input, cache_write_5m, cache_write_1h, cache_read, output]
}

fn assert_prices(actual: [f64; 5], expected: [f64; 5], context: &str) {
    for (actual, expected) in actual.iter().zip(expected) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "{context}: {actual:?} instead of {expected:?}"
        );
    }
}

#[test]
fn every_model_on_the_pricing_page_costs_what_the_page_says() {
    let table = PriceTable::bundled();
    for (id, name, prices) in PAGE {
        assert_eq!(table.model_name(id), Some(name));
        assert_prices(rates(id, each(10_000)), prices, id);

        let cost = table.cost(&request(id, each(10_000))).unwrap();
        let [input, five_minutes, one_hour, read, output] = prices;
        assert_prices(
            [
                cost.input,
                cost.cache_write_5m,
                cost.cache_write_1h,
                cost.cache_read,
                cost.output,
            ],
            [
                input / 100.0,
                five_minutes / 100.0,
                one_hour / 100.0,
                read / 100.0,
                output / 100.0,
            ],
            id,
        );
        assert!(
            (cost.total() - prices.iter().sum::<f64>() / 100.0).abs() < 1e-9,
            "{id}: the total is the sum of its parts"
        );
    }
}

#[test]
fn the_table_says_where_its_prices_come_from() {
    let table = PriceTable::bundled();
    assert_eq!(
        table.source(),
        "https://platform.claude.com/docs/en/about-claude/pricing"
    );
    assert_eq!(table.checked().to_string(), "2026-10-10");
}

#[test]
fn a_typical_request_costs_the_sum_of_its_parts() {
    let tokens = Tokens {
        input: 3,
        output: 640,
        cache_read: 42_000,
        cache_write_5m: 500,
        cache_write_1h: 1_000,
    };
    let cost = PriceTable::bundled()
        .cost(&request("claude-opus-5-5", tokens))
        .unwrap();

    // 3 × $4 + 640 × $20 + 42,000 × $0.20 + 500 × $5 + 1,000 × $8, per million tokens.
    assert!((cost.total() - 0.031_712).abs() < 1e-12, "{}", cost.total());
}

#[test]
fn haiku_5_5_charges_more_for_prompts_over_100000_tokens() {
    let at_threshold = Tokens {
        input: 40_000,
        cache_read: 50_000,
        cache_write_5m: 6_000,
        cache_write_1h: 4_000,
        output: 900,
    };
    let over = Tokens {
        input: at_threshold.input + 1,
        ..at_threshold
    };

    assert_prices(
        rates("claude-haiku-5-5", at_threshold),
        PAGE[17].2,
        "100,000",
    );
    assert_prices(
        rates("claude-haiku-5-5", over),
        HAIKU_5_5_LONG_PROMPT,
        "100,001",
    );
}

#[test]
fn fast_mode_multiplies_the_fast_input_price_for_caching() {
    let fast = |model| {
        let rates = PriceTable::bundled()
            .rates(&Request {
                speed: Some("fast"),
                ..request(model, each(1))
            })
            .unwrap();
        [
            rates.input,
            rates.cache_write_5m,
            rates.cache_write_1h,
            rates.cache_read,
            rates.output,
        ]
    };

    assert_prices(
        fast("claude-opus-5-5"),
        [8.0, 10.0, 16.0, 0.4, 40.0],
        "Opus 5.5",
    );
    assert_prices(
        fast("claude-opus-5"),
        [10.0, 12.5, 20.0, 1.0, 50.0],
        "Opus 5",
    );
    assert_prices(
        fast("claude-opus-4-8"),
        [10.0, 12.5, 20.0, 1.0, 50.0],
        "Opus 4.8",
    );
    // Claude Opus 4.6 runs fast-mode requests at standard speed and standard prices.
    assert_prices(fast("claude-opus-4-6"), PAGE[8].2, "Opus 4.6");
}

#[test]
fn us_only_inference_costs_a_tenth_more_on_every_token() {
    let us = |model, tokens| {
        let rates = PriceTable::bundled()
            .rates(&Request {
                inference_geo: Some("us"),
                ..request(model, tokens)
            })
            .unwrap();
        [
            rates.input,
            rates.cache_write_5m,
            rates.cache_write_1h,
            rates.cache_read,
            rates.output,
        ]
    };

    assert_prices(
        us("claude-sonnet-5-5", each(1)),
        [2.2, 2.75, 4.4, 0.11, 11.0],
        "Sonnet 5.5",
    );
    assert_prices(
        us("claude-haiku-5-5", each(100_000)),
        [0.55, 0.6875, 1.1, 0.055, 2.75],
        "Haiku 5.5, long prompt",
    );
    for geo in [None, Some("global"), Some("not_available")] {
        let rates = PriceTable::bundled()
            .rates(&Request {
                inference_geo: geo,
                ..request("claude-sonnet-5-5", each(1))
            })
            .unwrap();
        assert!((rates.input - 2.0).abs() < 1e-12, "{geo:?}");
    }
}

#[test]
fn web_searches_cost_ten_dollars_a_thousand_wherever_they_run() {
    for geo in [None, Some("us")] {
        let cost = PriceTable::bundled()
            .cost(&Request {
                web_search_requests: 3,
                inference_geo: geo,
                ..request("claude-opus-5-5", Tokens::default())
            })
            .unwrap();
        assert!((cost.web_search - 0.03).abs() < 1e-12, "{geo:?}");
        assert!((cost.total() - 0.03).abs() < 1e-12, "{geo:?}");
    }
}

#[test]
fn recorded_model_ids_find_their_prices() {
    for (recorded, name) in [
        ("claude-haiku-4-5-20251001", "Claude Haiku 4.5"),
        ("claude-sonnet-4-5-20250929", "Claude Sonnet 4.5"),
        ("claude-opus-4-20250514", "Claude Opus 4"),
        ("us.anthropic.claude-opus-5-5", "Claude Opus 5.5"),
        ("claude-haiku-4-5@20251001", "Claude Haiku 4.5"),
        ("claude-opus-4-6[1m]", "Claude Opus 4.6"),
    ] {
        assert_eq!(
            PriceTable::bundled().model_name(recorded),
            Some(name),
            "{recorded}"
        );
    }
}

#[test]
fn a_model_missing_from_the_table_is_unpriced_never_free() {
    let table = PriceTable::bundled();
    for model in ["claude-orion-1", "glm-4.6", "<synthetic>", ""] {
        assert_eq!(table.model_name(model), None, "{model}");
        assert_eq!(table.cost(&request(model, each(1_000))), None, "{model}");
    }
}

#[test]
fn a_table_names_its_format_first() {
    let with_format = |format: &str| {
        PriceTable::parse(&format!(
            r#"{{
                {format}
                "source": "https://example.com/pricing",
                "checked": "2026-10-10",
                "web_search_per_thousand": 10,
                "us_only_inference_multiplier": 1.1,
                "models": []
            }}"#
        ))
    };

    assert!(with_format("").is_ok());
    assert!(with_format(r#""format": 1,"#).is_ok());
    assert!(matches!(
        with_format(r#""format": 0,"#),
        Err(PriceTableError::Invalid(_))
    ));
    assert!(matches!(
        PriceTable::parse(r#"{ "format": 2, "prices": "in a shape this version cannot know" }"#),
        Err(PriceTableError::NewerFormat(2))
    ));
}

fn table_with(models: &str) -> Result<PriceTable, PriceTableError> {
    PriceTable::parse(&format!(
        r#"{{
            "source": "https://example.com/pricing",
            "checked": "2026-10-10",
            "web_search_per_thousand": 10,
            "us_only_inference_multiplier": 1.1,
            "models": [{models}]
        }}"#
    ))
}

const PRICES: &str =
    r#""input": 2, "cache_write_5m": 2.5, "cache_write_1h": 4, "cache_read": 0.2, "output": 10"#;

#[test]
fn a_price_change_applies_from_its_date_and_history_keeps_the_old_price() {
    let table = table_with(&format!(
        r#"{{ "name": "Claude Sonnet 5", "ids": ["claude-sonnet-5"], "prices": [
            {{ {PRICES} }},
            {{ "from": "2026-09-01", "input": 3, "cache_write_5m": 3.75, "cache_write_1h": 6,
               "cache_read": 0.3, "output": 15 }}
        ] }}"#
    ))
    .unwrap();
    let input_price = |timestamp: &str| {
        let at: jiff::Timestamp = timestamp.parse().unwrap();
        table
            .rates(&Request {
                timestamp_ms: at.as_millisecond(),
                ..request("claude-sonnet-5", each(1))
            })
            .unwrap()
            .input
    };

    assert!((input_price("2026-08-31T23:59:59.999Z") - 2.0).abs() < 1e-12);
    assert!((input_price("2026-09-01T00:00:00Z") - 3.0).abs() < 1e-12);
}

#[test]
fn a_table_that_breaks_its_rules_is_refused() {
    let invalid = [
        format!(
            r#"{{ "name": "A", "ids": ["a"], "prices": [{{ {PRICES} }}] }},
               {{ "name": "B", "ids": ["a"], "prices": [{{ {PRICES} }}] }}"#
        ),
        format!(r#"{{ "name": "A", "ids": [], "prices": [{{ {PRICES} }}] }}"#),
        r#"{ "name": "A", "ids": ["a"], "prices": [] }"#.to_owned(),
        format!(
            r#"{{ "name": "A", "ids": ["a"], "prices": [{{ "from": "2026-01-01", {PRICES} }}] }}"#
        ),
        format!(r#"{{ "name": "A", "ids": ["a"], "prices": [{{ {PRICES} }}, {{ {PRICES} }}] }}"#),
        format!(
            r#"{{ "name": "A", "ids": ["a"], "prices": [{{ {PRICES} }},
                {{ "from": "2026-05-01", {PRICES} }}, {{ "from": "2026-04-01", {PRICES} }}] }}"#
        ),
        r#"{ "name": "A", "ids": ["a"], "prices": [{ "input": -1, "cache_write_5m": 0,
            "cache_write_1h": 0, "cache_read": 0, "output": 0 }] }"#
            .to_owned(),
        r#"{ "name": "A", "ids": ["a"], "prices": [{ "input": 0, "cache_write_5m": 0,
            "cache_write_1h": 0, "cache_read": 0, "output": 0 }] }"#
            .to_owned(),
        format!(
            r#"{{ "name": "A", "ids": ["a"], "prices": [{{ {PRICES}, "fast": {{ "input": 4, "output": 20 }},
                "long_prompt": {{ "above": 100000, {PRICES} }} }}] }}"#
        ),
        format!(
            r#"{{ "name": "A", "ids": ["a"], "prices": [{{ {PRICES}, "cache_wrte_1h": 4 }}] }}"#
        ),
    ];
    for models in invalid {
        assert!(table_with(&models).is_err(), "{models}");
    }
    assert!(
        table_with(&format!(
            r#"{{ "name": "A", "ids": ["a"], "prices": [{{ {PRICES} }}] }}"#
        ))
        .is_ok()
    );
}
