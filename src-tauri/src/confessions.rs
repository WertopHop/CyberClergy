//! Persistence for the confessional queue.
//!
//! Entries live in `app_config_dir/confessions.json`, beside the settings, and
//! never leave the device: only the direction derived from an entry is put into
//! the prompt. The file is written atomically and, on Unix, readable by its
//! owner only.
//!
//! A file that cannot be parsed is not overwritten silently. The original is
//! copied to `confessions.json.corrupt` first, so a person who hand-edited the
//! file (or a disk that lost a sector) can still recover their words.

use std::fs;
use std::path::{Path, PathBuf};

use cyberclergy_llm::IntercessionQueue;

use crate::prefs::write_json_atomic;

pub const CONFESSIONS_FILE: &str = "confessions.json";

#[derive(Debug)]
pub struct ConfessionStore {
    path: PathBuf,
    queue: IntercessionQueue,
}

impl ConfessionStore {
    /// Reads the queue, adopts the current cleric count and makes sure the file
    /// exists, so the first launch already has something to grow.
    pub fn open(path: impl Into<PathBuf>, clerics: usize) -> Self {
        let path = path.into();
        let mut queue = load(&path).unwrap_or_else(|| IntercessionQueue::new(clerics));
        queue.set_clerics(clerics);
        let store = Self { path, queue };
        if let Err(error) = store.persist() {
            eprintln!("cannot write the confessional file: {error}");
        }
        store
    }

    /// Where the queue is stored. Read by the startup banner of a debug build,
    /// so a release build carries the accessor without a caller.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn queue(&self) -> &IntercessionQueue {
        &self.queue
    }

    pub fn queue_mut(&mut self) -> &mut IntercessionQueue {
        &mut self.queue
    }

    pub fn persist(&self) -> Result<(), String> {
        write_json_atomic(&self.path, &self.queue)
    }
}

fn load(path: &Path) -> Option<IntercessionQueue> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            eprintln!("cannot read {}: {error}", path.display());
            return None;
        }
    };
    match serde_json::from_str::<IntercessionQueue>(&text) {
        Ok(queue) => Some(queue),
        Err(error) => {
            let backup = path.with_extension("json.corrupt");
            let _ = fs::copy(path, &backup);
            eprintln!(
                "{} is unreadable ({error}); the original is kept at {}",
                path.display(),
                backup.display()
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let directory = std::env::temp_dir()
            .join(format!("cyberclergy-confessions-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        directory.join(CONFESSIONS_FILE)
    }

    #[test]
    fn entries_and_counters_survive_a_restart() {
        let path = temp_path("round-trip");
        let mut store = ConfessionStore::open(&path, 4);
        assert!(store.path().exists(), "the file is created on first open");

        let entry = store
            .queue_mut()
            .submit("я виноват перед мамой, прости меня", 1_000)
            .expect("valid entry")
            .entry;
        store
            .queue_mut()
            .record_prayer(&entry.id, 2_000);
        store.persist().expect("persisted");

        let reopened = ConfessionStore::open(&path, 4);
        let restored = reopened
            .queue()
            .get(&entry.id)
            .expect("the entry is restored");
        assert_eq!(restored.prayers_done, 1);
        assert_eq!(reopened.queue().clerics(), 4);

        let _ = fs::remove_dir_all(path.parent().unwrap_or(&path));
    }

    #[test]
    fn changing_the_cleric_count_keeps_assignments_valid() {
        let path = temp_path("clerics");
        let mut store = ConfessionStore::open(&path, 8);
        let entry = store
            .queue_mut()
            .submit("одиноко и никто не звонит", 1)
            .expect("valid")
            .entry;
        assert!(entry.cleric < 8);
        store.persist().expect("persisted");

        let reopened = ConfessionStore::open(&path, 3);
        let cleric = reopened.queue().get(&entry.id).expect("entry exists").cleric;
        assert!(cleric < 3, "the slot was re-mapped to the smaller cloud");

        let _ = fs::remove_dir_all(path.parent().unwrap_or(&path));
    }

    #[test]
    fn a_corrupted_file_is_backed_up_before_it_is_replaced() {
        let path = temp_path("corrupt");
        fs::create_dir_all(path.parent().unwrap()).expect("directory is created");
        fs::write(&path, "{ this is not a queue").expect("corrupt file is written");

        let store = ConfessionStore::open(&path, 5);
        assert_eq!(store.queue().entries().len(), 0);
        assert!(
            path.with_extension("json.corrupt").exists(),
            "the unreadable original is preserved"
        );

        let _ = fs::remove_dir_all(path.parent().unwrap_or(&path));
    }
}
