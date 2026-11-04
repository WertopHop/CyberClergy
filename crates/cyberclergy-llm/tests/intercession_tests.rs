//! Behaviour of the confessional queue: classification, assignment, counters
//! and persistence. The network layer is not involved here on purpose.

use cyberclergy_llm::{
    target_for, IntercessionQueue, IntercessionStatus, Severity, Theme, MAX_TEXT_CHARS,
};

const GUILT: &str = "я виноват перед мамой, прости меня, я соврал";
const ILLNESS: &str = "болит спина, врач не знает что это";
const WORK: &str = "не могу больше тянуть работу и долги";

#[test]
fn entry_without_text_is_rejected() {
    let mut queue = IntercessionQueue::new(3);
    assert!(queue.submit("   ", 1).is_err());
    assert!(queue.active_count() == 0);
}

#[test]
fn oversized_entry_is_rejected() {
    let mut queue = IntercessionQueue::new(3);
    let text = "а".repeat(MAX_TEXT_CHARS + 1);
    let error = queue.submit(&text, 1).expect_err("must be rejected");
    assert!(error.to_string().contains(&MAX_TEXT_CHARS.to_string()));
}

#[test]
fn entry_is_classified_and_kept_local() {
    let mut queue = IntercessionQueue::new(3);
    let outcome = queue.submit(GUILT, 1_000).expect("entry is valid");
    let entry = outcome.entry;

    assert_eq!(entry.theme, Theme::Guilt);
    assert_eq!(entry.status, IntercessionStatus::Queued);
    assert!(entry.direction.contains("guilt"));
    // The provider sees the direction only; the confession stays in the entry.
    assert!(!entry.direction.contains("мам"));
    assert!(entry.text.contains("мам"));
    assert_eq!(entry.prayers_target, target_for(entry.severity, GUILT.chars().count()));
}

#[test]
fn duplicate_entry_is_not_queued_twice() {
    let mut queue = IntercessionQueue::new(3);
    let first = queue.submit(GUILT, 1_000).expect("first is valid");
    let second = queue
        .submit("  я виноват перед мамой, прости меня, я соврал ", 2_000)
        .expect("duplicate is accepted");

    assert!(second.duplicate);
    assert_eq!(second.entry.id, first.entry.id);
    assert_eq!(queue.active_count(), 1);
}

#[test]
fn entries_are_spread_over_the_clergy() {
    let mut queue = IntercessionQueue::new(3);
    queue.submit(GUILT, 1).expect("valid");
    queue.submit(ILLNESS, 2).expect("valid");
    queue.submit(WORK, 3).expect("valid");

    let assignments: Vec<usize> = queue.entries().iter().map(|entry| entry.cleric).collect();
    assert_eq!(assignments, vec![0, 1, 2]);

    // The fourth entry goes to the least loaded cleric, starting from the
    // rotation point.
    queue.submit("одиноко и никто не звонит", 4).expect("valid");
    assert_eq!(queue.pending_count(0), 2);
}

#[test]
fn pressing_entries_are_served_first() {
    let mut queue = IntercessionQueue::new(1);
    let calm = queue
        .submit("хочу просто поблагодарить за этот год, спасибо", 100)
        .expect("valid");
    let urgent = queue
        .submit("умер папа, я не знаю как жить дальше", 200)
        .expect("valid");

    assert_eq!(calm.entry.severity, Severity::Gentle);
    assert_eq!(urgent.entry.severity, Severity::Urgent);

    let task = queue.next_task(&[]).expect("a task is due");
    assert_eq!(task.id, urgent.entry.id);
    assert_eq!(task.ordinal, 1);
    assert_eq!(task.target, 7);
}

#[test]
fn busy_clerics_are_skipped() {
    let mut queue = IntercessionQueue::new(2);
    let entry = queue.submit(GUILT, 1).expect("valid").entry;

    assert!(queue.next_task(&[entry.cleric]).is_none());
    assert!(queue.next_task(&[]).is_some());
    // Only the other cleric being busy leaves the entry available.
    let other = (entry.cleric + 1) % 2;
    assert!(queue.next_task(&[other]).is_some());
}

#[test]
fn prayers_are_counted_until_the_target() {
    let mut queue = IntercessionQueue::new(1);
    let entry = queue.submit(ILLNESS, 1).expect("valid").entry.clone();
    assert!(entry.prayers_target >= 3);

    for index in 0..entry.prayers_target {
        let outcome = queue
            .record_prayer(&entry.id, 10 + i64::from(index))
            .expect("entry is active");
        assert_eq!(outcome.done, index + 1);
        assert_eq!(outcome.fulfilled, index + 1 == entry.prayers_target);
    }

    let fulfilled = queue.get(&entry.id).expect("entry is stored");
    assert_eq!(fulfilled.status, IntercessionStatus::Fulfilled);
    assert_eq!(fulfilled.remaining(), 0);
    assert!(queue.next_task(&[]).is_none());
    assert!(queue.record_prayer(&entry.id, 99).is_none());
}

#[test]
fn cancelled_entry_leaves_the_queue() {
    let mut queue = IntercessionQueue::new(1);
    let entry = queue.submit(GUILT, 1).expect("valid").entry;

    assert!(queue.cancel(&entry.id));
    assert!(!queue.cancel(&entry.id));
    assert_eq!(queue.active_count(), 0);
    assert_eq!(
        queue.get(&entry.id).map(|entry| entry.status),
        Some(IntercessionStatus::Cancelled)
    );
    assert!(queue.next_task(&[]).is_none());
}

#[test]
fn fewer_clerics_keeps_assignments_valid() {
    let mut queue = IntercessionQueue::new(7);
    queue.submit(GUILT, 1).expect("valid");
    queue.submit(ILLNESS, 2).expect("valid");

    queue.set_clerics(2);
    assert!(queue
        .active()
        .all(|entry| entry.cleric < 2));
}

#[test]
fn queue_survives_a_json_round_trip() {
    let mut queue = IntercessionQueue::new(4);
    let entry = queue.submit(GUILT, 1).expect("valid").entry;
    queue.record_prayer(&entry.id, 2);

    let json = serde_json::to_string(&queue).expect("serialisable");
    let restored: IntercessionQueue = serde_json::from_str(&json).expect("deserialisable");

    assert_eq!(restored, queue);
    assert_eq!(restored.clerics(), 4);
    assert_eq!(restored.get(&entry.id).map(|entry| entry.prayers_done), Some(1));
}

#[test]
fn finished_entries_are_pruned_but_active_ones_stay() {
    let mut queue = IntercessionQueue::new(1);
    let mut ids = Vec::new();
    for index in 0..3 {
        let entry = queue
            .submit(&format!("{WORK} {index}"), i64::from(index))
            .expect("valid")
            .entry;
        for prayer in 0..entry.prayers_target {
            queue.record_prayer(&entry.id, 100 + i64::from(prayer));
        }
        ids.push(entry.id);
    }
    let active = queue.submit("одиноко и никто не звонит", 500).expect("valid").entry;

    let removed = queue.prune_finished(1);
    assert_eq!(removed, 2);
    assert_eq!(queue.entries().len(), 2);
    assert!(queue.get(&active.id).is_some());
    assert!(queue.get(&ids[0]).is_none());
    assert!(queue.get(&ids[2]).is_some());
}

#[test]
fn target_grows_with_severity_and_length() {
    assert_eq!(target_for(Severity::Gentle, 40), 3);
    assert_eq!(target_for(Severity::Steady, 200), 5);
    assert_eq!(target_for(Severity::Urgent, 200), 7);
    assert_eq!(target_for(Severity::Steady, 500), 6);
    assert_eq!(target_for(Severity::Urgent, 900), 8);
}
