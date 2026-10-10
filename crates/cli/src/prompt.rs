//! Asking before a change.

use std::io::{IsTerminal, Write};

use crate::style::Paint;

/// Asks `question` before a change, unless `yes` already confirmed it. Without a terminal to
/// ask on, nothing changes.
pub fn confirmed(paint: Paint, yes: bool, question: &str) -> bool {
    if yes {
        return true;
    }
    if !std::io::stdin().is_terminal() {
        eprintln!(
            "\nNothing was changed. Run the command again with --yes to make the change without a question."
        );
        return false;
    }
    print!("\n{question} {} ", paint.strong("[y/N]"));
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    let _ = std::io::stdin().read_line(&mut answer);
    let yes = matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes");
    if !yes {
        println!("Nothing was changed.");
    }
    yes
}
