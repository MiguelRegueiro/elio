use super::*;
use rusqlite::{Connection, OpenFlags, types::ValueRef};
use std::{
    ffi::OsString,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

const SQLITE_MAGIC: &[u8; 16] = b"SQLite format 3\x00";
const MAX_TABLES: usize = 40;
const MAX_COLUMNS: usize = 20;
const SAMPLE_ROWS: usize = 3;
const MAX_CELL_WIDTH: usize = 30;

struct SqliteHeader {
    page_size: u32,
    text_encoding: u32,
    write_version: u8,
    sqlite_version: u32,
}

struct SchemaObject {
    kind: String,
    name: String,
}

struct ColumnInfo {
    name: String,
    type_name: String,
    not_null: bool,
    is_pk: bool,
    is_hidden: bool,
    is_generated: bool,
}

pub(in crate::preview) fn build_sqlite_preview(path: &Path) -> Option<PreviewContent> {
    let raw = read_header_bytes(path)?;

    if &raw[..16] != SQLITE_MAGIC {
        return None;
    }

    let header = parse_header(&raw);
    let conn = open_preview_connection(path, header.write_version == 2)?;

    let palette = theme::palette();
    let mut lines = Vec::new();

    let summary: Vec<(&str, Option<String>)> = vec![
        ("Version", header.version_label()),
        ("Page size", Some(format!("{}", header.page_size))),
        ("Encoding", Some(header.encoding_label().to_string())),
        ("Journal", Some(header.journal_label().to_string())),
    ];
    push_data_section(&mut lines, "Details", &summary, palette);

    let objects = query_schema_objects(&conn).unwrap_or_default();
    let tables: Vec<_> = objects.iter().filter(|o| o.kind == "table").collect();
    let views: Vec<_> = objects.iter().filter(|o| o.kind == "view").collect();

    if !tables.is_empty() {
        lines.push(Line::from(""));
        let heading = if tables.len() == 1 {
            "Table".to_string()
        } else {
            format!("Tables ({})", tables.len())
        };
        lines.push(section_line(&heading, palette));

        for (i, table) in tables.iter().take(MAX_TABLES).enumerate() {
            lines.push(Line::from(""));
            lines.push(object_name_line(&table.name, palette));

            let columns = query_table_columns(&conn, &table.name);
            let visible_cols: Vec<_> = columns.iter().filter(|c| !c.is_hidden).collect();

            // Width of the widest type name among the columns we'll display,
            // so that PK / NOT NULL / NULL / GENERATED tags align vertically.
            let type_width = visible_cols
                .iter()
                .take(MAX_COLUMNS)
                .map(|c| c.type_name.chars().count())
                .max()
                .unwrap_or(0);

            // A true INTEGER PRIMARY KEY rowid alias has no explicit primary-key
            // index entry in index_list. INTEGER PRIMARY KEY DESC (and composite
            // or other non-alias forms) do produce one, so we use this flag to
            // decide whether to suppress the null badge for the PK column.
            let has_explicit_pk_index = table_has_explicit_pk_index(&conn, &table.name);

            for col in visible_cols.iter().take(MAX_COLUMNS) {
                lines.push(column_line(col, type_width, has_explicit_pk_index, palette));
            }
            if visible_cols.len() > MAX_COLUMNS {
                lines.push(muted_line(
                    &format!("  … {} more columns", visible_cols.len() - MAX_COLUMNS),
                    palette,
                ));
            }

            if i == 0
                && !visible_cols.is_empty()
                && let Some(sample) =
                    render_sample_rows(&conn, &table.name, &visible_cols, SAMPLE_ROWS, palette)
            {
                lines.push(Line::from(""));
                lines.extend(sample);
            }
        }

        if tables.len() > MAX_TABLES {
            lines.push(Line::from(""));
            lines.push(muted_line(
                &format!("  … {} more tables not shown", tables.len() - MAX_TABLES),
                palette,
            ));
        }
    }

    if !views.is_empty() {
        lines.push(Line::from(""));
        let heading = if views.len() == 1 {
            "View".to_string()
        } else {
            format!("Views ({})", views.len())
        };
        lines.push(section_line(&heading, palette));
        for view in views.iter().take(MAX_TABLES) {
            lines.push(Line::from(""));
            lines.push(object_name_line(&view.name, palette));
        }
    }

    if objects.is_empty() {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        lines.push(muted_line("No tables or views", palette));
    }

    Some(PreviewContent::new(PreviewKind::Data, lines).with_detail("SQLite database"))
}

fn read_header_bytes(path: &Path) -> Option<[u8; 100]> {
    let mut buf = [0u8; 100];
    File::open(path).ok()?.read_exact(&mut buf).ok()?;
    Some(buf)
}

fn open_preview_connection(path: &Path, is_wal: bool) -> Option<Connection> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    if !is_wal {
        return Connection::open_with_flags(path, flags).ok();
    }

    // SQLite opens sidecars beside the target database, not beside a symlink.
    let path = path.canonicalize().ok()?;
    let wal_exists = sidecar_path(&path, "-wal").try_exists().ok()?;
    let shm_exists = sidecar_path(&path, "-shm").try_exists().ok()?;

    match (wal_exists, shm_exists) {
        // A partial sidecar set would make SQLite create the missing file.
        (true, false) | (false, true) => None,
        // Existing sidecars may contain committed data that immutable mode ignores.
        (true, true) => Connection::open_with_flags(&path, flags).ok(),
        // With no sidecars, immutable mode avoids creating empty WAL/SHM files.
        (false, false) => {
            Connection::open_with_flags(immutable_uri(&path)?, flags | OpenFlags::SQLITE_OPEN_URI)
                .ok()
        }
    }
}

fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut sidecar = OsString::from(path.as_os_str());
    sidecar.push(suffix);
    PathBuf::from(sidecar)
}

fn immutable_uri(path: &Path) -> Option<String> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let mut uri = String::from("file:");
        append_uri_encoded(&mut uri, path.as_os_str().as_bytes());
        uri.push_str("?immutable=1");
        Some(uri)
    }

    #[cfg(windows)]
    {
        let mut normalized = path.to_str()?.replace('\\', "/");
        if let Some(path) = normalized.strip_prefix("//?/UNC/") {
            normalized = format!("//{path}");
        } else if let Some(path) = normalized.strip_prefix("//?/") {
            normalized = path.to_string();
        }

        let mut uri = if let Some(unc) = normalized.strip_prefix("//") {
            let mut uri = String::from("file:////");
            append_uri_encoded(&mut uri, unc.as_bytes());
            uri
        } else if normalized.as_bytes().get(1) == Some(&b':')
            && normalized.as_bytes().get(2) == Some(&b'/')
            && normalized.as_bytes()[0].is_ascii_alphabetic()
        {
            let mut uri = String::from("file:/");
            append_uri_encoded(&mut uri, normalized.as_bytes());
            uri
        } else {
            return None;
        };
        uri.push_str("?immutable=1");
        Some(uri)
    }

    #[cfg(not(any(unix, windows)))]
    {
        let mut uri = String::from("file:");
        append_uri_encoded(&mut uri, path.to_str()?.as_bytes());
        uri.push_str("?immutable=1");
        Some(uri)
    }
}

fn append_uri_encoded(uri: &mut String, bytes: &[u8]) {
    for &byte in bytes {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/' | b':') {
            uri.push(byte as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            uri.push('%');
            uri.push(HEX[(byte >> 4) as usize] as char);
            uri.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
}

fn parse_header(raw: &[u8; 100]) -> SqliteHeader {
    let page_size_raw = u16::from_be_bytes([raw[16], raw[17]]);
    let page_size = if page_size_raw == 1 {
        65536u32
    } else {
        page_size_raw as u32
    };
    let text_encoding = u32::from_be_bytes([raw[56], raw[57], raw[58], raw[59]]);
    // Bytes 96-99: version of SQLite that last wrote the file.
    // Encoded as major*1_000_000 + minor*1_000 + release.
    let sqlite_version = u32::from_be_bytes([raw[96], raw[97], raw[98], raw[99]]);
    SqliteHeader {
        page_size,
        text_encoding,
        write_version: raw[18],
        sqlite_version,
    }
}

impl SqliteHeader {
    fn encoding_label(&self) -> &'static str {
        match self.text_encoding {
            2 => "UTF-16 LE",
            3 => "UTF-16 BE",
            _ => "UTF-8",
        }
    }

    fn journal_label(&self) -> &'static str {
        if self.write_version == 2 {
            "WAL"
        } else {
            "Rollback"
        }
    }

    fn version_label(&self) -> Option<String> {
        if self.sqlite_version == 0 {
            return None;
        }
        let major = self.sqlite_version / 1_000_000;
        let minor = (self.sqlite_version % 1_000_000) / 1_000;
        let release = self.sqlite_version % 1_000;
        Some(format!("{major}.{minor}.{release}"))
    }
}

fn query_schema_objects(conn: &Connection) -> Option<Vec<SchemaObject>> {
    let mut stmt = conn
        .prepare(
            "SELECT type, name FROM sqlite_schema \
             WHERE type IN ('table', 'view') \
             AND name NOT LIKE 'sqlite_%' \
             ORDER BY type DESC, name",
        )
        .ok()?;

    let objects = stmt
        .query_map([], |row| {
            Ok(SchemaObject {
                kind: row.get::<_, String>(0)?,
                name: row.get::<_, String>(1)?,
            })
        })
        .ok()?
        .filter_map(|r| r.ok())
        .collect();

    Some(objects)
}

fn table_has_explicit_pk_index(conn: &Connection, table_name: &str) -> bool {
    // PRAGMA index_list returns one row per index. The `origin` column (index 3)
    // is 'pk' for indexes created by a PRIMARY KEY constraint. A true INTEGER
    // PRIMARY KEY rowid alias never generates such an entry; INTEGER PRIMARY KEY
    // DESC, composite PKs, and other non-alias forms do.
    let escaped = table_name.replace('"', "\"\"");
    let query = format!("PRAGMA index_list(\"{escaped}\")");
    let Ok(mut stmt) = conn.prepare(&query) else {
        return false;
    };
    stmt.query_map([], |row| row.get::<_, String>(3))
        .map(|rows| rows.filter_map(|r| r.ok()).any(|origin| origin == "pk"))
        .unwrap_or(false)
}

fn query_table_columns(conn: &Connection, table_name: &str) -> Vec<ColumnInfo> {
    let escaped = table_name.replace('"', "\"\"");
    let query = format!("PRAGMA table_xinfo(\"{escaped}\")");
    let mut stmt = match conn.prepare(&query) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    stmt.query_map([], |row| {
        // cid, name, type, notnull, dflt_value, pk, hidden
        // hidden=0: normal; 1: virtual-table internal (suppress);
        // 2: GENERATED ALWAYS VIRTUAL; 3: GENERATED ALWAYS STORED.
        let hidden = row.get::<_, i64>(6).unwrap_or(0);
        Ok(ColumnInfo {
            name: row.get::<_, String>(1)?,
            type_name: row.get::<_, String>(2).unwrap_or_default(),
            not_null: row.get::<_, i64>(3).unwrap_or(0) != 0,
            is_pk: row.get::<_, i64>(5).unwrap_or(0) > 0,
            is_hidden: hidden == 1,
            is_generated: matches!(hidden, 2 | 3),
        })
    })
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

fn render_sample_rows(
    conn: &Connection,
    table_name: &str,
    columns: &[&ColumnInfo],
    limit: usize,
    palette: theme::Palette,
) -> Option<Vec<Line<'static>>> {
    let col_list = columns
        .iter()
        .map(|c| format!("\"{}\"", c.name.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(", ");
    let escaped_table = table_name.replace('"', "\"\"");
    let query = format!("SELECT {col_list} FROM \"{escaped_table}\" LIMIT {limit}");

    let mut stmt = conn.prepare(&query).ok()?;
    let col_count = stmt.column_count();

    let rows: Vec<Vec<String>> = stmt
        .query_map([], |row| {
            let mut cells = Vec::with_capacity(col_count);
            for i in 0..col_count {
                let cell = match row.get_ref(i) {
                    Ok(ValueRef::Null) => "NULL".to_string(),
                    Ok(ValueRef::Integer(n)) => n.to_string(),
                    Ok(ValueRef::Real(f)) => format!("{f}"),
                    Ok(ValueRef::Text(b)) => {
                        let s = String::from_utf8_lossy(b);
                        if s.chars().count() > MAX_CELL_WIDTH {
                            format!(
                                "{}…",
                                s.chars().take(MAX_CELL_WIDTH - 1).collect::<String>()
                            )
                        } else {
                            s.into_owned()
                        }
                    }
                    Ok(ValueRef::Blob(b)) => format!("<blob {} B>", b.len()),
                    Err(_) => "?".to_string(),
                };
                cells.push(cell);
            }
            Ok(cells)
        })
        .ok()?
        .filter_map(|r| r.ok())
        .collect();

    if rows.is_empty() {
        return None;
    }

    // Compute column widths from header names and data values
    let mut col_widths: Vec<usize> = columns.iter().map(|c| c.name.chars().count()).collect();
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            if i < col_widths.len() {
                col_widths[i] = col_widths[i].max(cell.chars().count());
            }
        }
    }
    for w in &mut col_widths {
        *w = (*w).min(MAX_CELL_WIDTH);
    }

    let mut lines = Vec::new();

    // Header
    let header_parts: Vec<String> = columns
        .iter()
        .zip(&col_widths)
        .map(|(col, w)| format!("{:<width$}", col.name, width = w))
        .collect();
    lines.push(Line::from(Span::styled(
        format!("  {}", header_parts.join("  ")),
        Style::default().fg(palette.muted),
    )));

    // Data rows
    for row in &rows {
        let row_parts: Vec<String> = row
            .iter()
            .zip(&col_widths)
            .map(|(cell, w)| format!("{:<width$}", cell, width = w))
            .collect();
        lines.push(Line::from(Span::styled(
            format!("  {}", row_parts.join("  ")),
            Style::default().fg(palette.text),
        )));
    }

    Some(lines)
}

fn object_name_line(name: &str, palette: theme::Palette) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {name}"),
        Style::default().fg(palette.text),
    ))
}

fn column_line(
    col: &ColumnInfo,
    type_width: usize,
    table_has_explicit_pk_index: bool,
    palette: theme::Palette,
) -> Line<'static> {
    let mut spans = Vec::new();

    spans.push(Span::styled(
        format!("    {:<24}", col.name),
        Style::default().fg(palette.text),
    ));

    // Type field: padded to the widest type in this table so the constraint
    // badges that follow are vertically aligned across all column rows.
    if type_width > 0 {
        spans.push(Span::styled(
            format!(
                "{:<width$}",
                col.type_name.to_uppercase(),
                width = type_width
            ),
            Style::default().fg(palette.muted),
        ));
    }

    if col.is_pk {
        spans.push(Span::styled(
            " PK".to_string(),
            Style::default().fg(palette.accent),
        ));
    }

    // Show a nullability badge for every column, with one exception:
    // a true INTEGER PRIMARY KEY rowid alias — inserting NULL auto-assigns a
    // fresh rowid, so the column never actually stores NULL even though
    // table_xinfo reports notnull=0. Showing NULL for it would be misleading.
    //
    // A rowid alias requires ALL of:
    //   • declared type exactly "INTEGER" (case-insensitive; INT/BIGINT etc. do not qualify)
    //   • no explicit pk index in index_list — that rules out INTEGER PRIMARY KEY DESC
    //     and composite PKs, which are not rowid aliases
    //   • notnull=0 — if the user wrote NOT NULL we surface it either way
    let is_rowid_alias = col.is_pk
        && col.type_name.eq_ignore_ascii_case("INTEGER")
        && !col.not_null
        && !table_has_explicit_pk_index;

    if !is_rowid_alias {
        if col.not_null {
            spans.push(Span::styled(
                " NOT NULL".to_string(),
                Style::default().fg(palette.muted),
            ));
        } else {
            spans.push(Span::styled(
                " NULL".to_string(),
                Style::default().fg(palette.muted),
            ));
        }
    }

    // Generated columns carry an extra tag so they are visually distinct from
    // plain stored columns with the same type and constraint.
    if col.is_generated {
        spans.push(Span::styled(
            "  GENERATED".to_string(),
            Style::default().fg(palette.muted),
        ));
    }

    Line::from(spans)
}

fn muted_line(text: &str, palette: theme::Palette) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::default().fg(palette.muted),
    ))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn immutable_uri_handles_windows_drive_and_unc_paths() {
        assert_eq!(
            immutable_uri(Path::new(r"C:\dbs\a #1%.db")).as_deref(),
            Some("file:/C:/dbs/a%20%231%25.db?immutable=1")
        );
        assert_eq!(
            immutable_uri(Path::new(r"\\server\share\a.db")).as_deref(),
            Some("file:////server/share/a.db?immutable=1")
        );
        assert!(immutable_uri(Path::new(r"C:relative.db")).is_none());
    }
}
