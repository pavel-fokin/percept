//! Presentation layer: the command-line interface.

use std::path::Path;
use std::process::ExitCode;

use crate::app::{self, AppService};
use crate::core::EventStore;

pub async fn run<S: EventStore>(service: &AppService<S>, dir: &Path, home: &Path) -> ExitCode {
    match service.event_count().await {
        Ok(count) => {
            let source = app::source_path(dir).await;
            println!(
                "percept • {} • {}",
                display_path(&source, home),
                status_line(count)
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("percept: {error}");
            ExitCode::FAILURE
        }
    }
}

fn display_path(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
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
    fn display_path_abbreviates_home() {
        let home = Path::new("/home/me");
        assert_eq!(display_path(Path::new("/home/me/code/x"), home), "~/code/x");
        assert_eq!(display_path(home, home), "~");
        assert_eq!(display_path(Path::new("/tmp/x"), home), "/tmp/x");
        assert_eq!(
            display_path(Path::new("/home/mexico"), home),
            "/home/mexico"
        );
    }

    #[test]
    fn status_line_phrasings() {
        assert_eq!(status_line(0), "Nothing recorded yet.");
        assert_eq!(status_line(1), "1 event recorded.");
        assert_eq!(status_line(3), "3 events recorded.");
    }
}
