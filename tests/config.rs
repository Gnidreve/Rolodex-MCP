//! Integrationstests für `sendmail_mcp::config::load_contacts` über die
//! öffentliche Lib-API - kein Netzwerk, keine Kanal-ENV-Variablen nötig,
//! da nur E-Mail (rein formale Validierung) als Kanal registriert ist.

use sendmail_mcp::config::load_contacts;

fn write_config(contents: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, contents).expect("config schreiben");
    (dir, path)
}

#[test]
fn missing_file_produces_readable_error() {
    let err = load_contacts(std::path::Path::new("/nicht/vorhanden/config.toml")).unwrap_err();
    assert!(err.to_string().contains("konnte config nicht lesen"));
}

#[test]
fn empty_contact_list_is_rejected() {
    let (_dir, path) = write_config("");
    let err = load_contacts(&path).unwrap_err();
    assert!(err.to_string().contains("keinen einzigen [[to]]-Eintrag"));
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
    assert!(err.to_string().contains("leeren name"));
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
    let tools = load_contacts(&path).expect("gültige Config");
    assert_eq!(tools.len(), 2);
    let names: Vec<&str> = tools.iter().map(|t| t.tool_name.as_str()).collect();
    assert!(names.contains(&"send_to_max_mustermann_via_email"));
    assert!(names.contains(&"send_to_erika_musterfrau_via_email"));
}

#[test]
fn blank_channel_value_counts_as_not_configured() {
    // Ein leerer String (nach trim()) für ein Kanal-Feld zählt nicht als
    // konfigurierter Kanal - ohne einen anderen Kanal muss das wie "gar
    // kein Kanal" behandelt werden und der Server-Start ablehnen.
    let (_dir, path) = write_config(
        r#"
[[to]]
name = "Max Mustermann"
email = "   "
"#,
    );
    let err = load_contacts(&path).unwrap_err();
    assert!(err.to_string().contains("keinen einzigen Kanal"));
}
