use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

use crate::config::{group_by_channel, load_contacts, ContactEntry};

/// Polls config.toml's modification time and reloads the contact book when
/// it changes, without restarting the process. Deliberately mtime-polling
/// instead of an inotify/FSEvents-based watcher (e.g. the `notify` crate):
/// those file-system event watchers are unreliable across Docker bind
/// mounts - the project's primary deployment path via Coolify - especially
/// on macOS Docker Desktop and network-backed volumes. Polling a file's
/// mtime works identically everywhere.
///
/// Only the contact *data* is reloaded - the set of active channels (and
/// therefore the tool list itself) stays fixed for the process lifetime,
/// since a channel's sender is only built and connection-tested once at
/// startup. A config.toml edit that references a channel with no active
/// sender is accepted but its contacts are ignored (logged), and needs a
/// restart to take effect.
pub fn spawn(
    path: PathBuf,
    active_channels: Vec<&'static str>,
    contacts: Arc<RwLock<HashMap<&'static str, Vec<ContactEntry>>>>,
    poll_interval: Duration,
) {
    tokio::spawn(async move {
        let mut last_modified = modified_at(&path);
        loop {
            tokio::time::sleep(poll_interval).await;

            let modified = modified_at(&path);
            if modified == last_modified {
                continue;
            }
            last_modified = modified;

            match load_contacts(&path) {
                Ok(entries) => {
                    let mut grouped = group_by_channel(entries);
                    let ignored: Vec<&'static str> = grouped
                        .keys()
                        .copied()
                        .filter(|slug| !active_channels.contains(slug))
                        .collect();
                    if !ignored.is_empty() {
                        tracing::warn!(
                            "config.toml now references channel(s) {} with no active sender - \
                             a newly used channel needs a server restart, ignoring their \
                             contacts for now",
                            ignored.join(", ")
                        );
                    }
                    grouped.retain(|slug, _| active_channels.contains(slug));

                    *contacts.write().expect("lock not poisoned") = grouped;
                    tracing::info!("config.toml changed - contact book reloaded");
                }
                Err(err) => {
                    tracing::warn!("config.toml changed but failed to reload ({err:#}) - keeping the previous contact book");
                }
            }
        }
    });
}

fn modified_at(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn picks_up_a_changed_config_without_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[[to]]\nname = \"Max Mustermann\"\nemail = \"max@example.com\"\n").unwrap();

        let contacts = Arc::new(RwLock::new(group_by_channel(load_contacts(&path).unwrap())));
        spawn(path.clone(), vec!["email"], contacts.clone(), Duration::from_millis(20));

        assert_eq!(contacts.read().unwrap().get("email").map(Vec::len), Some(1));

        // A fresh mtime needs a real filesystem tick on some systems -
        // sleeping first, then writing, avoids a flaky "same mtime" miss.
        tokio::time::sleep(Duration::from_millis(50)).await;
        std::fs::write(
            &path,
            "[[to]]\nname = \"Max Mustermann\"\nemail = \"max@example.com\"\n\n[[to]]\nname = \"Erika Musterfrau\"\nemail = \"erika@example.com\"\n",
        )
        .unwrap();

        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(contacts.read().unwrap().get("email").map(Vec::len), Some(2));
    }

    #[tokio::test]
    async fn keeps_the_previous_contact_book_on_a_broken_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[[to]]\nname = \"Max Mustermann\"\nemail = \"max@example.com\"\n").unwrap();

        let contacts = Arc::new(RwLock::new(group_by_channel(load_contacts(&path).unwrap())));
        spawn(path.clone(), vec!["email"], contacts.clone(), Duration::from_millis(20));

        tokio::time::sleep(Duration::from_millis(50)).await;
        std::fs::write(&path, "this is not valid toml {{{").unwrap();

        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(contacts.read().unwrap().get("email").map(Vec::len), Some(1));
    }
}
