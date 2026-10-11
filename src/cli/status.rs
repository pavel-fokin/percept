use super::Result;
use crate::app::AppService;
use crate::core::EventStore;

pub(super) async fn status<S: EventStore>(service: &AppService<S>) -> Result<()> {
    println!("percept • {}", status_line(service.event_count().await?));
    Ok(())
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
