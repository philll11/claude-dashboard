//! Which series each model belongs to, as every surface colours them (ADR 0007): each family's
//! newest model in the history gets the family's colour, and every other model shares the series
//! of other models.

use std::collections::HashMap;

use claude_dashboard_server::protocol::ModelRow;

use crate::tui::theme::Series;

/// A model id's family and version, such as `claude-opus-5-5` for Opus 5.5.
fn parse(model: &str) -> Option<(Series, (u32, u32))> {
    for (name, series) in [
        ("fable", Series::Fable),
        ("opus", Series::Opus),
        ("sonnet", Series::Sonnet),
        ("haiku", Series::Haiku),
    ] {
        let prefix = format!("claude-{name}-");
        let Some(at) = model.find(&prefix) else {
            continue;
        };
        let rest = &model[at + prefix.len()..];
        let major: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let Ok(major_number) = major.parse::<u32>() else {
            continue;
        };
        let minor = rest[major.len()..]
            .strip_prefix('-')
            .map(|tail| {
                tail.chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
            })
            .filter(|digits| (1..=2).contains(&digits.len()))
            .and_then(|digits| digits.parse::<u32>().ok())
            .unwrap_or(0);
        return Some((series, (major_number, minor)));
    }
    None
}

/// The series of each of `models`.
pub fn series_of<'a>(models: impl IntoIterator<Item = &'a str>) -> HashMap<String, Series> {
    let models: Vec<&str> = models.into_iter().collect();
    let mut newest: HashMap<Series, (&str, (u32, u32))> = HashMap::new();
    for model in &models {
        let Some((series, version)) = parse(model) else {
            continue;
        };
        if newest.get(&series).is_none_or(|(_, best)| version > *best) {
            newest.insert(series, (model, version));
        }
    }
    models
        .iter()
        .map(|model| {
            let series = parse(model)
                .map(|(series, _)| series)
                .filter(|series| newest.get(series).is_some_and(|(best, _)| best == model))
                .unwrap_or(Series::Other);
            ((*model).to_owned(), series)
        })
        .collect()
}

/// What each series is called in a legend: its model's name, or `Other models` for several.
#[derive(Debug, Default)]
pub struct SeriesNames(HashMap<Series, String>);

impl SeriesNames {
    pub fn of(rows: &[ModelRow], series: &HashMap<String, Series>) -> Self {
        let mut names: HashMap<Series, String> = HashMap::new();
        for row in rows {
            let of = series.get(&row.model.id).copied().unwrap_or(Series::Other);
            names
                .entry(of)
                .and_modify(|name| "Other models".clone_into(name))
                .or_insert_with(|| crate::tui::canvas::clean(&row.model.name));
        }
        Self(names)
    }

    pub fn get(&self, series: Series) -> &str {
        self.0.get(&series).map_or("Other models", String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_familys_newest_model_takes_its_colour() {
        let series = series_of([
            "claude-opus-5-5",
            "claude-opus-5",
            "claude-sonnet-4-5-20250929",
            "claude-opus-4-20250514",
            "glm-4.6",
        ]);
        assert_eq!(series["claude-opus-5-5"], Series::Opus);
        assert_eq!(series["claude-opus-5"], Series::Other);
        assert_eq!(series["claude-opus-4-20250514"], Series::Other);
        assert_eq!(series["claude-sonnet-4-5-20250929"], Series::Sonnet);
        assert_eq!(series["glm-4.6"], Series::Other);
    }
}
