use super::*;

#[test]
fn a_fresh_id_is_minted_between_before_and_after() {
    let before = Timestamp::now();
    let id: Id<()> = Id::new();
    let after = Timestamp::now();

    let minted = id.minted_at().unwrap();
    // UUIDv7 keeps milliseconds, so it may sit a fraction before `before`.
    assert!(minted <= after);
    assert!(minted >= before.minus_minutes(1).unwrap());
}

#[test]
fn an_id_of_another_uuid_version_has_no_minting_time() {
    let id: Id<()> = Id::from_uuid(Uuid::nil());
    assert_eq!(id.minted_at(), None);
}
