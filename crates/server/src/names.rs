//! What the reports and the dashboard call projects and models.

use std::collections::HashMap;

use claude_dashboard_core::history::{History, ProjectKey};

use crate::app::App;

/// What reports call a project: the last part of its working folder, with as many parts before it
/// as tell it apart from every other project's, or else its transcript folder.
pub fn project_names(history: &History) -> HashMap<ProjectKey, (String, Option<String>, String)> {
    let projects = history.projects().unwrap_or_default();
    let names = short_names(
        &projects
            .iter()
            .map(|project| project.path.as_deref())
            .collect::<Vec<_>>(),
    );
    projects
        .into_iter()
        .zip(names)
        .map(|(project, name)| {
            let name = name.unwrap_or_else(|| project.transcript_folder.clone());
            (project.key, (name, project.path, project.transcript_folder))
        })
        .collect()
}

/// Each folder's last part, with as many parts before it as tell it apart from the others.
fn short_names(paths: &[Option<&str>]) -> Vec<Option<String>> {
    let parts: Vec<Vec<&str>> = paths
        .iter()
        .map(|path| {
            path.map_or_else(Vec::new, |path| {
                path.trim_end_matches(['/', '\\'])
                    .rsplit(['/', '\\'])
                    .collect()
            })
        })
        .collect();
    parts
        .iter()
        .enumerate()
        .map(|(index, own)| {
            if own.is_empty() {
                return None;
            }
            let mut count = 1;
            while count < own.len()
                && parts.iter().enumerate().any(|(other, theirs)| {
                    other != index && theirs.get(..count) == own.get(..count)
                })
            {
                count += 1;
            }
            let mut name: Vec<&str> = own[..count].to_vec();
            name.reverse();
            Some(name.join("/"))
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

#[cfg(test)]
mod tests {
    use super::short_names;

    #[test]
    fn a_project_is_named_by_as_much_of_its_folder_as_tells_it_apart() {
        let names = short_names(&[
            Some(r"C:\code\claude-dashboard"),
            Some(r"C:\code\api\workspace"),
            Some(r"C:\bench\matrix\A-fathom-1\workspace"),
            Some(r"C:\bench\smoke\A-fathom-1\workspace\"),
            Some("/home/you/notes"),
            None,
        ]);
        assert_eq!(
            names,
            [
                Some("claude-dashboard".to_owned()),
                Some("api/workspace".to_owned()),
                Some("matrix/A-fathom-1/workspace".to_owned()),
                Some("smoke/A-fathom-1/workspace".to_owned()),
                Some("notes".to_owned()),
                None,
            ]
        );
    }

    #[test]
    fn the_same_folder_twice_keeps_its_whole_path() {
        let names = short_names(&[Some(r"C:\code\app"), Some(r"C:\code\app")]);
        assert_eq!(
            names,
            [
                Some("C:/code/app".to_owned()),
                Some("C:/code/app".to_owned())
            ]
        );
    }
}
