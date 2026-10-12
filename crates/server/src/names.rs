//! What the reports and the dashboard call projects and models.

use std::collections::HashMap;

use claude_dashboard_core::history::{History, ProjectKey};

use crate::app::App;

/// What reports call a project: the last part of its working folder, or the last two parts when
/// another project ends in the same folder name, or else its transcript folder.
pub fn project_names(history: &History) -> HashMap<ProjectKey, (String, Option<String>, String)> {
    let projects = history.projects().unwrap_or_default();
    let tail = |path: &str, parts: usize| {
        let parts: Vec<&str> = path
            .trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\'])
            .take(parts)
            .collect();
        parts.into_iter().rev().collect::<Vec<_>>().join("/")
    };
    let mut seen: HashMap<String, usize> = HashMap::new();
    for project in &projects {
        if let Some(path) = &project.path {
            *seen.entry(tail(path, 1)).or_default() += 1;
        }
    }
    projects
        .into_iter()
        .map(|project| {
            let name = project.path.as_deref().map_or_else(
                || project.transcript_folder.clone(),
                |path| {
                    let short = tail(path, 1);
                    if seen.get(&short).copied().unwrap_or(0) > 1 {
                        tail(path, 2)
                    } else {
                        short
                    }
                },
            );
            (project.key, (name, project.path, project.transcript_folder))
        })
        .collect()
}

/// A model's short name, such as `Opus 5.5`, or its id when the price table does not know it.
#[must_use]
pub fn model_name(app: &App, model: &str) -> String {
    app.prices.model_name(model).map_or_else(
        || model.to_owned(),
        |name| name.strip_prefix("Claude ").unwrap_or(name).to_owned(),
    )
}
