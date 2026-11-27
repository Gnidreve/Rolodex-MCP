//! Integration tests for `sendmail_mcp::config::load_contacts` via the
//! public lib API - no network, no channel ENV variables needed, since
//! only email (pure format validation) is registered as a channel.

use sendmail_mcp::config::load_contacts;

fn write_config(contents: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, contents).expect("writing config");
    (dir, path)
}

#[test]
fn missing_file_produces_readable_error() {
    let err = load_contacts(std::path::Path::new("/does/not/exist/config.toml")).unwrap_err();
    assert!(err.to_string().contains("could not read config"));
}

#[test]
fn empty_contact_list_is_rejected() {
    let (_dir, path) = write_config("");
    let err = load_contacts(&path).unwrap_err();
    assert!(err.to_string().contains("not contain a single [[to]] entry"));
}

#[test]
fn blank_name_is_rejected() {
    let (_dir, path) = write_config(
        r#"
[[to]]
name = "   "
email = "max@example.com"
"#,
    );
    let err = load_contacts(&path).unwrap_err();
    assert!(err.to_string().contains("empty name"));
}

#[test]
fn multiple_contacts_with_one_channel_each_produce_distinct_tools() {
    let (_dir, path) = write_config(
        r#"
[[to]]
name = "Max Mustermann"
email = "max@example.com"

[[to]]
name = "Erika Musterfrau"
email = "erika@example.com"
"#,
    );
    let tools = load_contacts(&path).expect("valid config");
    assert_eq!(tools.len(), 2);
    let names: Vec<&str> = tools.iter().map(|t| t.tool_name.as_str()).collect();
    assert!(names.contains(&"send_to_max_mustermann_via_email"));
    assert!(names.contains(&"send_to_erika_musterfrau_via_email"));
}

#[test]
fn blank_channel_value_counts_as_not_configured() {
    // An empty string (after trim()) for a channel field doesn't count as
    // a configured channel - without another channel this must be treated
    // as "no channel at all" and reject the server start.
    let (_dir, path) = write_config(
        r#"
[[to]]
name = "Max Mustermann"
email = "   "
"#,
    );
    let err = load_contacts(&path).unwrap_err();
    assert!(err.to_string().contains("not a single channel"));
}
