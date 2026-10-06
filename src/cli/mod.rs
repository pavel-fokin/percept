//! Presentation layer: the command-line interface.

use std::process::ExitCode;

use crate::app::AppService;
use crate::core::EventStore;

/// Prints the status line, or the error to stderr with exit code 1.
pub fn run<S: EventStore>(service: &AppService<S>) -> ExitCode {
    match service.event_count() {
        Ok(count) => {
            println!("{}", status_line(count));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("percept: {error}");
            ExitCode::FAILURE
        }
    }
}

fn status_line(count: usize) -> String {
    match count {
        0 => "percept \u{2022} Nothing recorded yet.".to_string(),
        1 => "percept \u{2022} 1 event recorded.".to_string(),
        n => format!("percept \u{2022} {n} events recorded."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_line_phrasings() {
        assert_eq!(status_line(0), "percept • Nothing recorded yet.");
        assert_eq!(status_line(1), "percept • 1 event recorded.");
        assert_eq!(status_line(3), "percept • 3 events recorded.");
    }
}
