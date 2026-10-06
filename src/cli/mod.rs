//! Presentation layer: the command-line interface.

use std::process::ExitCode;

use crate::app::AppService;

pub fn run(service: &AppService) -> ExitCode {
    match service.event_count() {
        Ok(count) => {
            println!("percept • {}", status_line(count));
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
        0 => "Nothing recorded yet.".to_string(),
        1 => "1 event recorded.".to_string(),
        n => format!("{n} events recorded."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_line_phrasings() {
        assert_eq!(status_line(0), "Nothing recorded yet.");
        assert_eq!(status_line(1), "1 event recorded.");
        assert_eq!(status_line(3), "3 events recorded.");
    }
}
