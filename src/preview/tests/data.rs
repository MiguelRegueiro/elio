use super::*;
use rusqlite::Connection;
use std::{fs, io::Write};

// ── SQLite ────────────────────────────────────────────────────────────────────

#[test]
fn sqlite_preview_shows_header_and_tables() {
    let root = temp_path("sqlite-basic");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("app.sqlite");

    let conn = Connection::open(&path).expect("failed to open sqlite db");
    conn.execute_batch(
        "CREATE TABLE accounts (
             id    INTEGER PRIMARY KEY,
             name  TEXT NOT NULL,
             email TEXT
         );
         CREATE TABLE posts (
             id         INTEGER PRIMARY KEY,
             account_id INTEGER NOT NULL,
             body       TEXT
         );",
    )
    .expect("failed to create tables");
    // Insert into `accounts`, which sorts before `posts` alphabetically and will
    // be the first table shown, triggering sample-row rendering.
    conn.execute(
        "INSERT INTO accounts (name, email) VALUES (?1, ?2)",
        ["Alice", "alice@example.com"],
    )
    .expect("failed to insert row");
    drop(conn);

    let entry = file_entry(path.clone());
    let preview = build_preview(&entry);

    let text: Vec<String> = preview.lines().iter().map(line_text).collect();

    assert_eq!(preview.kind, PreviewKind::Data);
    assert_eq!(preview.detail.as_deref(), Some("SQLite database"));

    // Details section header
    assert!(
        text.iter().any(|l| l == "Details"),
        "expected 'Details' section header; got: {text:?}"
    );
    // Page size field
    assert!(
        text.iter().any(|l| l.contains("Page size")),
        "expected 'Page size' field; got: {text:?}"
    );
    // Both tables listed
    assert!(
        text.iter().any(|l| l.contains("accounts")),
        "expected 'accounts' table; got: {text:?}"
    );
    assert!(
        text.iter().any(|l| l.contains("posts")),
        "expected 'posts' table; got: {text:?}"
    );
    // Column names and constraint badges for accounts table.
    // INTEGER PRIMARY KEY is a rowid alias — must show PK but no null badge.
    assert!(
        text.iter()
            .any(|l| l.trim_start().starts_with("id ") && l.contains("PK")),
        "expected 'id' column with PK badge; got: {text:?}"
    );
    assert!(
        !text
            .iter()
            .any(|l| l.trim_start().starts_with("id ") && l.contains("NULL")),
        "INTEGER PRIMARY KEY 'id' must not carry a NULL or NOT NULL badge; got: {text:?}"
    );
    assert!(
        text.iter()
            .any(|l| l.contains("name") && l.contains("NOT NULL")),
        "expected 'name' column with NOT NULL badge; got: {text:?}"
    );
    assert!(
        text.iter()
            .any(|l| l.contains("email") && l.contains("NULL")),
        "expected 'email' column with NULL badge; got: {text:?}"
    );
    // Sample row value
    assert!(
        text.iter().any(|l| l.contains("Alice")),
        "expected sample row with 'Alice'; got: {text:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn sqlite_preview_does_not_create_wal_sidecars() {
    let root = temp_path("sqlite-wal-no-sidecars");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("cash #100%.sqlite");
    let conn = Connection::open(&path).expect("failed to open sqlite db");
    conn.pragma_update(None, "journal_mode", "WAL")
        .expect("failed to enable WAL mode");
    conn.execute("CREATE TABLE items (name TEXT NOT NULL)", [])
        .expect("failed to create table");
    drop(conn);

    let wal = root.join("cash #100%.sqlite-wal");
    let shm = root.join("cash #100%.sqlite-shm");
    assert!(!wal.exists());
    assert!(!shm.exists());

    let preview = build_preview(&file_entry(path));
    let text: Vec<String> = preview.lines().iter().map(line_text).collect();
    assert!(text.iter().any(|line| line.contains("items")), "{text:?}");
    assert!(!wal.exists(), "SQLite preview created {}", wal.display());
    assert!(!shm.exists(), "SQLite preview created {}", shm.display());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn sqlite_preview_handles_non_utf8_wal_paths_without_sidecars() {
    use std::os::unix::ffi::OsStringExt;

    let root = temp_path("sqlite-wal-non-utf8");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join(std::ffi::OsString::from_vec(b"cash-\xff.sqlite".to_vec()));
    let conn = Connection::open(&path).expect("failed to open sqlite db");
    conn.pragma_update(None, "journal_mode", "WAL")
        .expect("failed to enable WAL mode");
    conn.execute("CREATE TABLE items (name TEXT NOT NULL)", [])
        .expect("failed to create table");
    drop(conn);

    let preview = build_preview(&file_entry(path.clone()));
    let text: Vec<String> = preview.lines().iter().map(line_text).collect();
    assert!(text.iter().any(|line| line.contains("items")), "{text:?}");
    let mut wal = path.as_os_str().to_os_string();
    wal.push("-wal");
    let mut shm = path.as_os_str().to_os_string();
    shm.push("-shm");
    assert!(!std::path::PathBuf::from(wal).exists());
    assert!(!std::path::PathBuf::from(shm).exists());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn sqlite_preview_reads_active_wal_and_rejects_partial_sidecars() {
    let root = temp_path("sqlite-active-wal");
    let source_dir = root.join("source");
    let copy_dir = root.join("copy");
    fs::create_dir_all(&source_dir).expect("failed to create source dir");
    fs::create_dir_all(&copy_dir).expect("failed to create copy dir");
    let source = source_dir.join("source.sqlite");
    let conn = Connection::open(&source).expect("failed to open sqlite db");
    conn.pragma_update(None, "journal_mode", "WAL")
        .expect("failed to enable WAL mode");
    conn.pragma_update(None, "wal_autocheckpoint", 0)
        .expect("failed to disable checkpoints");
    conn.execute_batch(
        "CREATE TABLE messages (body TEXT NOT NULL);
         INSERT INTO messages VALUES ('committed in WAL');",
    )
    .expect("failed to populate database");

    let preview = build_preview(&file_entry(source.clone()));
    let text: Vec<String> = preview.lines().iter().map(line_text).collect();
    assert!(
        text.iter().any(|line| line.contains("committed in WAL")),
        "active WAL data was omitted: {text:?}"
    );

    let copy = copy_dir.join("copy.sqlite");
    let copy_wal = copy_dir.join("copy.sqlite-wal");
    let copy_shm = copy_dir.join("copy.sqlite-shm");
    fs::copy(&source, &copy).expect("failed to copy database");
    fs::copy(source_dir.join("source.sqlite-wal"), &copy_wal).expect("failed to copy WAL");
    assert!(crate::preview::data::build_sqlite_preview(&copy).is_none());
    assert!(!copy_shm.exists(), "preview created the missing SHM");

    fs::remove_file(&copy_wal).expect("failed to remove WAL");
    fs::copy(source_dir.join("source.sqlite-shm"), &copy_shm).expect("failed to copy SHM");
    assert!(crate::preview::data::build_sqlite_preview(&copy).is_none());
    assert!(!copy_wal.exists(), "preview created the missing WAL");

    drop(conn);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[cfg(unix)]
#[test]
fn sqlite_preview_reads_active_wal_through_symlink() {
    use std::os::unix::fs::symlink;

    let root = temp_path("sqlite-wal-symlink");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let source = root.join("source.sqlite");
    let link = root.join("linked.sqlite");
    let conn = Connection::open(&source).expect("failed to open sqlite db");
    conn.pragma_update(None, "journal_mode", "WAL")
        .expect("failed to enable WAL mode");
    conn.pragma_update(None, "wal_autocheckpoint", 0)
        .expect("failed to disable checkpoints");
    conn.execute_batch(
        "CREATE TABLE messages (body TEXT NOT NULL);
         INSERT INTO messages VALUES ('visible through symlink');",
    )
    .expect("failed to populate database");
    symlink(&source, &link).expect("failed to create symlink");

    let preview = build_preview(&file_entry(link));
    let text: Vec<String> = preview.lines().iter().map(line_text).collect();
    assert!(
        text.iter()
            .any(|line| line.contains("visible through symlink")),
        "symlink preview omitted WAL data: {text:?}"
    );
    assert!(!root.join("linked.sqlite-wal").exists());
    assert!(!root.join("linked.sqlite-shm").exists());

    drop(conn);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn sqlite_preview_shows_views() {
    let root = temp_path("sqlite-view");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("app.sqlite");

    let conn = Connection::open(&path).expect("failed to open sqlite db");
    conn.execute_batch(
        "CREATE TABLE items (id INTEGER PRIMARY KEY, value TEXT);
         CREATE VIEW active_items AS SELECT * FROM items WHERE value IS NOT NULL;",
    )
    .expect("failed to create schema");
    drop(conn);

    let entry = file_entry(path.clone());
    let preview = build_preview(&entry);
    let text: Vec<String> = preview.lines().iter().map(line_text).collect();

    assert_eq!(preview.kind, PreviewKind::Data);
    assert!(
        text.iter().any(|l| l.contains("active_items")),
        "expected view name; got: {text:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn non_sqlite_db_file_falls_through_to_binary_preview() {
    let root = temp_path("sqlite-not-sqlite");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("custom.db");
    // Write a file that is clearly not SQLite (no magic bytes).
    fs::write(&path, b"\x00\x01\x02\x03not sqlite at all\x00").expect("failed to write file");

    let entry = file_entry(path.clone());
    let preview = build_preview(&entry);

    // Must NOT produce a Data/SQLite preview.
    assert_ne!(
        preview.kind,
        PreviewKind::Data,
        "non-SQLite .db file should not get a Data preview"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn sqlite_preview_shows_generated_columns() {
    let root = temp_path("sqlite-generated");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("calc.sqlite");

    let conn = Connection::open(&path).expect("failed to open sqlite db");
    conn.execute_batch(
        "CREATE TABLE products (
             id        INTEGER PRIMARY KEY,
             price     REAL NOT NULL,
             tax_rate  REAL NOT NULL DEFAULT 0.2,
             -- VIRTUAL generated column (hidden = 2 in table_xinfo)
             price_inc REAL GENERATED ALWAYS AS (price * (1 + tax_rate)) VIRTUAL
         );",
    )
    .expect("failed to create table with generated column");
    drop(conn);

    let entry = file_entry(path.clone());
    let preview = build_preview(&entry);
    let text: Vec<String> = preview.lines().iter().map(line_text).collect();

    assert_eq!(preview.kind, PreviewKind::Data);
    assert!(
        text.iter().any(|l| l.contains("price_inc")),
        "generated column 'price_inc' should be visible; got: {text:?}"
    );
    assert!(
        text.iter()
            .any(|l| l.contains("price_inc") && l.contains("GENERATED")),
        "generated column 'price_inc' should carry the GENERATED badge; got: {text:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn sqlite_preview_shows_nullability_for_text_primary_key() {
    // TEXT PRIMARY KEY is nullable in SQLite — the column is NOT a rowid alias,
    // so notnull=0 in table_xinfo and NULL values are genuinely accepted.
    // The preview must show both the PK badge and the NULL badge for such columns,
    // rather than silently omitting nullability because the column is a PK.
    let root = temp_path("sqlite-nullable-pk");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("catalog.sqlite");

    let conn = Connection::open(&path).expect("failed to open sqlite db");
    conn.execute_batch(
        "CREATE TABLE entries (
             code     TEXT PRIMARY KEY,
             value    INTEGER NOT NULL,
             note     TEXT
         );",
    )
    .expect("failed to create table");
    drop(conn);

    let entry = file_entry(path.clone());
    let preview = build_preview(&entry);
    let text: Vec<String> = preview.lines().iter().map(line_text).collect();

    assert_eq!(preview.kind, PreviewKind::Data);

    // TEXT PRIMARY KEY: is_pk=true, notnull=0 → must show both PK and NULL.
    assert!(
        text.iter().any(|l| l.contains("code") && l.contains("PK")),
        "expected 'code' to show PK badge; got: {text:?}"
    );
    assert!(
        text.iter()
            .any(|l| l.contains("code") && l.contains("NULL") && !l.contains("NOT NULL")),
        "expected 'code' TEXT PRIMARY KEY to show NULL (not NOT NULL); got: {text:?}"
    );

    // Sanity-check the other columns.
    assert!(
        text.iter()
            .any(|l| l.contains("value") && l.contains("NOT NULL")),
        "expected 'value' to show NOT NULL; got: {text:?}"
    );
    assert!(
        text.iter()
            .any(|l| l.contains("note") && l.contains("NULL") && !l.contains("NOT NULL")),
        "expected 'note' to show NULL; got: {text:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn sqlite_preview_shows_null_for_integer_pk_desc() {
    // INTEGER PRIMARY KEY DESC is not a rowid alias in SQLite — it creates an
    // explicit primary-key index and the column genuinely accepts NULL values.
    // The preview must show both PK and NULL, not just PK.
    let root = temp_path("sqlite-int-pk-desc");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("app.sqlite");

    let conn = Connection::open(&path).expect("failed to open sqlite db");
    conn.execute_batch(
        "CREATE TABLE items (
             id    INTEGER PRIMARY KEY DESC,
             label TEXT NOT NULL
         );",
    )
    .expect("failed to create table");
    drop(conn);

    let entry = file_entry(path.clone());
    let preview = build_preview(&entry);
    let text: Vec<String> = preview.lines().iter().map(line_text).collect();

    assert_eq!(preview.kind, PreviewKind::Data);
    assert!(
        text.iter()
            .any(|l| l.trim_start().starts_with("id ") && l.contains("PK")),
        "expected PK badge for 'id'; got: {text:?}"
    );
    // DESC prevents the rowid alias — the column can hold NULL, so the preview
    // must show the NULL badge rather than silently hiding nullability.
    assert!(
        text.iter().any(|l| l.trim_start().starts_with("id ")
            && l.contains("NULL")
            && !l.contains("NOT NULL")),
        "INTEGER PRIMARY KEY DESC must show NULL badge; got: {text:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

// ── CSV ───────────────────────────────────────────────────────────────────────

#[test]
fn csv_preview_renders_aligned_table_with_detected_header() {
    let root = temp_path("csv-header");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("data.csv");
    fs::write(&path, "name,age,city\nAlice,28,New York\nBob,34,London\n")
        .expect("failed to write csv");

    let entry = file_entry(path.clone());
    let preview = build_preview(&entry);
    let text: Vec<String> = preview.lines().iter().map(line_text).collect();

    assert_eq!(preview.kind, PreviewKind::Data);
    assert_eq!(preview.detail.as_deref(), Some("CSV file"));

    // Header values present
    assert!(text.iter().any(|l| l.contains("name")), "{text:?}");
    assert!(text.iter().any(|l| l.contains("age")), "{text:?}");
    assert!(text.iter().any(|l| l.contains("city")), "{text:?}");
    // Data values present
    assert!(text.iter().any(|l| l.contains("Alice")), "{text:?}");
    assert!(text.iter().any(|l| l.contains("London")), "{text:?}");
    // Footer present
    assert!(text.iter().any(|l| l.contains("rows")), "{text:?}");

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn csv_preview_synthesizes_headers_for_all_text_data() {
    let root = temp_path("csv-no-header");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("words.csv");
    // All-text file — ambiguous, should get synthetic col1/col2 headers.
    fs::write(&path, "foo,bar\nbaz,qux\n").expect("failed to write csv");

    let entry = file_entry(path.clone());
    let preview = build_preview(&entry);
    let text: Vec<String> = preview.lines().iter().map(line_text).collect();

    assert_eq!(preview.kind, PreviewKind::Data);
    assert!(
        text.iter().any(|l| l.contains("col1")),
        "expected synthetic 'col1' header; got: {text:?}"
    );
    assert!(
        text.iter().any(|l| l.contains("col2")),
        "expected synthetic 'col2' header; got: {text:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn tsv_preview_uses_tab_delimiter() {
    let root = temp_path("tsv-basic");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("data.tsv");
    fs::write(&path, "product\tprice\nApple\t1.20\nBanana\t0.50\n").expect("failed to write tsv");

    let entry = file_entry(path.clone());
    let preview = build_preview(&entry);
    let text: Vec<String> = preview.lines().iter().map(line_text).collect();

    assert_eq!(preview.kind, PreviewKind::Data);
    assert_eq!(preview.detail.as_deref(), Some("TSV file"));
    assert!(text.iter().any(|l| l.contains("product")), "{text:?}");
    assert!(text.iter().any(|l| l.contains("Apple")), "{text:?}");

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn csv_preview_handles_quoted_fields_with_embedded_commas() {
    let root = temp_path("csv-quoted");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("places.csv");
    fs::write(
        &path,
        "city,country\n\"New York, NY\",USA\n\"London, UK\",UK\n",
    )
    .expect("failed to write csv");

    let entry = file_entry(path.clone());
    let preview = build_preview(&entry);
    let text: Vec<String> = preview.lines().iter().map(line_text).collect();

    assert_eq!(preview.kind, PreviewKind::Data);
    assert!(
        text.iter()
            .any(|l| l.contains("New York, NY") || l.contains("New York")),
        "expected quoted field content; got: {text:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn csv_preview_reports_64kib_truncation_for_large_file_with_few_rows() {
    let root = temp_path("csv-byte-truncated");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("big.csv");

    // Write a CSV with 5 data rows, each row padded to make the file > 64 KiB.
    // The header + 5 fat rows fit above 64 KiB so read_text_preview truncates
    // before row 50, yet the row count never hits the MAX_PREVIEW_ROWS cap.
    let padding = "x".repeat(14_000);
    let mut file = fs::File::create(&path).expect("failed to create csv");
    writeln!(file, "label,value,notes").expect("write header");
    for i in 1..=5u32 {
        writeln!(file, "row{i},{i},{padding}").expect("write row");
    }
    drop(file);

    let entry = file_entry(path.clone());
    let preview = build_preview(&entry);
    let text: Vec<String> = preview.lines().iter().map(line_text).collect();

    assert_eq!(preview.kind, PreviewKind::Data);
    // Footer must mention the 64 KiB cut, not a false row-cap message.
    assert!(
        text.iter().any(|l| l.contains("truncated at 64 KiB")),
        "expected '64 KiB' truncation note in footer; got: {text:?}"
    );
    assert!(
        !text.iter().any(|l| l.contains("more rows in file")),
        "must not claim 'more rows in file' when the cut was at 64 KiB; got: {text:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn csv_preview_reports_row_cap_truncation_for_file_with_many_short_rows() {
    let root = temp_path("csv-row-truncated");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("many.csv");

    // Write a CSV with 60 short rows — all fit within 64 KiB, but our cap is 50.
    let mut file = fs::File::create(&path).expect("failed to create csv");
    writeln!(file, "id,value").expect("write header");
    for i in 1..=60u32 {
        writeln!(file, "{i},{}", i * 10).expect("write row");
    }
    drop(file);

    let entry = file_entry(path.clone());
    let preview = build_preview(&entry);
    let text: Vec<String> = preview.lines().iter().map(line_text).collect();

    assert_eq!(preview.kind, PreviewKind::Data);
    // Footer must mention row cap, not 64 KiB.
    assert!(
        text.iter().any(|l| l.contains("more rows in file")),
        "expected 'more rows in file' note; got: {text:?}"
    );
    assert!(
        !text.iter().any(|l| l.contains("64 KiB")),
        "must not claim 64 KiB truncation when file fits in read window; got: {text:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}
