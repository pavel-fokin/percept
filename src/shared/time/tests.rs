use super::*;

#[test]
fn parse_time_parses_an_iso8601_timestamp() {
    let parsed = parse_time("2026-01-01T00:00:00Z").unwrap();
    assert_eq!(parsed.to_string(), "2026-01-01T00:00:00Z");
}

#[test]
fn parse_time_parses_relative_shorthand_as_a_time_in_the_past() {
    let now = Timestamp::now();
    for shorthand in ["1d", "2h", "30m"] {
        let parsed = parse_time(shorthand).unwrap();
        assert!(parsed < now, "{shorthand} should parse to before now");
    }
}

#[test]
fn parse_time_rejects_a_value_that_is_neither_form() {
    assert!(parse_time("3x").is_none());
}
