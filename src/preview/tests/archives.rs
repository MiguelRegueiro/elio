use super::*;
use crate::core::{EntryKind, FileClass};
use std::path::Path;

#[test]
fn torrent_preview_shows_single_file_metadata_and_trackers() {
    let root = temp_path("torrent");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("sample.torrent");
    let bytes = bencode_dict(vec![
        ("announce", bencode_str("https://tracker.test")),
        (
            "announce-list",
            bencode_list(vec![bencode_list(vec![
                bencode_str("https://tracker.test"),
                bencode_str("https://backup.test"),
            ])]),
        ),
        ("comment", bencode_str("test torrent")),
        ("created by", bencode_str("elio")),
        (
            "info",
            bencode_dict(vec![
                ("length", bencode_int(12_345)),
                ("name", bencode_str("file.txt")),
                ("piece length", bencode_int(262_144)),
                ("pieces", bencode_bytes(b"12345678901234567890")),
                ("private", bencode_int(1)),
            ]),
        ),
    ]);
    fs::write(&path, bytes).expect("failed to write torrent");

    let preview = build_preview(&file_entry(path));
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();

    assert_eq!(preview.kind, PreviewKind::Text);
    assert_eq!(preview.detail.as_deref(), Some("BitTorrent file"));
    assert_eq!(line_texts.first().map(String::as_str), Some("Details"));
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Name") && text.contains("file.txt"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Mode") && text.contains("Single-file"))
    );
    assert!(line_texts.iter().any(|text| text.contains("Private")));
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Trackers") && text.contains("2 across 1 tier"))
    );
    assert!(line_texts.iter().any(|text| text == "Trackers"));
    assert!(line_texts.iter().any(|text| {
        text.contains("Tier 1") && text.contains("tracker.test") && text.contains("backup.test")
    }));
    assert!(line_texts.iter().any(|text| text == "Contents"));
    assert!(line_texts.iter().any(|text| text.contains("file.txt")));
    assert!(!preview.truncated);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn torrent_preview_shows_multifile_contents_tree() {
    let root = temp_path("torrent-multifile");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("series.torrent");
    let bytes = bencode_dict(vec![
        (
            "announce-list",
            bencode_list(vec![
                bencode_list(vec![
                    bencode_str("https://tracker.one"),
                    bencode_str("https://tracker.two"),
                ]),
                bencode_list(vec![bencode_str("https://backup.tld/announce")]),
            ]),
        ),
        ("created by", bencode_str("elio")),
        (
            "info",
            bencode_dict(vec![
                (
                    "files",
                    bencode_list(vec![
                        bencode_dict(vec![
                            ("length", bencode_int(100)),
                            (
                                "path",
                                bencode_list(vec![
                                    bencode_str("season-01"),
                                    bencode_str("ep1.mkv"),
                                ]),
                            ),
                        ]),
                        bencode_dict(vec![
                            ("length", bencode_int(200)),
                            (
                                "path.utf-8",
                                bencode_list(vec![
                                    bencode_str("season-01"),
                                    bencode_str("ep2.mkv"),
                                ]),
                            ),
                        ]),
                    ]),
                ),
                ("name", bencode_str("series")),
                ("piece length", bencode_int(65_536)),
                (
                    "pieces",
                    bencode_bytes(b"1234567890123456789012345678901234567890"),
                ),
                ("private", bencode_int(0)),
            ]),
        ),
    ]);
    fs::write(&path, bytes).expect("failed to write torrent");

    let preview = build_preview(&file_entry(path));
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();

    assert_eq!(preview.kind, PreviewKind::Text);
    assert_eq!(preview.detail.as_deref(), Some("BitTorrent file"));
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Mode") && text.contains("Multi-file"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Files") && text.contains("2"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Trackers") && text.contains("3 across 2 tiers"))
    );
    assert!(line_texts.iter().any(|text| text.contains("Tier 2")));
    assert!(line_texts.iter().any(|text| text.contains("series/")));
    assert!(line_texts.iter().any(|text| text.contains("season-01/")));
    assert!(line_texts.iter().any(|text| text.contains("ep1.mkv")));
    assert!(line_texts.iter().any(|text| text.contains("ep2.mkv")));
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Privacy") && text.contains("Public"))
    );
    assert!(!preview.truncated);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn iso_binary_preview_keeps_specific_type_detail() {
    let root = temp_path("iso");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("disk.iso");
    fs::write(&path, [0x00, 0x81, 0xFE, 0xFF]).expect("failed to write iso");

    let preview = build_preview(&file_entry(path));

    assert_eq!(preview.kind, PreviewKind::Binary);
    assert_eq!(preview.detail.as_deref(), Some("ISO disk image"));

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn iso_metadata_parser_reads_primary_volume_descriptor() {
    let metadata = container::parse_iso_metadata(&sample_iso_descriptors())
        .expect("sample descriptors should parse");

    assert_eq!(metadata.system_id.as_deref(), Some("ELIO_SYS"));
    assert_eq!(metadata.volume_id.as_deref(), Some("ELIO_INSTALL"));
    assert_eq!(metadata.publisher_id.as_deref(), Some("Elio Publisher"));
    assert_eq!(metadata.preparer_id.as_deref(), Some("Elio Builder"));
    assert_eq!(metadata.application_id.as_deref(), Some("Elio Image Tool"));
    assert_eq!(metadata.created_at.as_deref(), Some("2026-03-11 09:00:00"));
    assert_eq!(metadata.modified_at.as_deref(), Some("2026-03-11 10:15:00"));
    assert_eq!(
        metadata.effective_at.as_deref(),
        Some("2026-03-12 00:00:00")
    );
    assert_eq!(
        metadata.total_size,
        Some(640 * container::ISO_SECTOR_SIZE as u64)
    );
    assert!(metadata.bootable);
}

#[test]
fn iso_entry_normalization_reconstructs_parents_and_strips_versions() {
    let entries = container::normalize_archive_entries(
        ["/docs/readme.txt;1", "./EFI/BOOT/", "boot.catalog;1"],
        true,
    );

    assert!(
        entries
            .iter()
            .any(|entry| entry.path == "docs" && entry.is_dir)
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry.path == "docs/readme.txt" && !entry.is_dir)
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry.path == "EFI" && entry.is_dir)
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry.path == "EFI/BOOT" && entry.is_dir)
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry.path == "boot.catalog" && !entry.is_dir)
    );
}

#[test]
fn iso_preview_renders_metadata_and_tree() {
    let preview = container::render_iso_preview(
        container::IsoMetadata {
            volume_id: Some("ELIO_INSTALL".to_string()),
            system_id: Some("ELIO_SYS".to_string()),
            total_size: Some(640 * container::ISO_SECTOR_SIZE as u64),
            bootable: true,
            created_at: Some("2026-03-11 09:00:00".to_string()),
            ..container::IsoMetadata::default()
        },
        container::normalize_archive_entries(
            ["boot/", "boot/grub/", "boot/grub/grub.cfg", "README.txt"],
            true,
        ),
    );
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();
    let header = preview
        .header_detail(0, 20)
        .expect("iso preview should expose header detail");

    assert_eq!(preview.kind, PreviewKind::Archive);
    assert_eq!(preview.detail.as_deref(), Some("ISO disk image"));
    assert!(header.contains("ISO disk image"));
    assert_eq!(line_texts.first().map(String::as_str), Some("Details"));
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Volume") && text.contains("ELIO_INSTALL"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text == "Contents" || text.ends_with("Contents"))
    );
    assert!(line_texts.iter().any(|text| text.contains("boot/")));
    assert!(line_texts.iter().any(|text| text.contains("grub.cfg")));
    assert!(line_texts.iter().any(|text| text.contains("README.txt")));
}

#[test]
fn iso_preview_reports_tree_truncation() {
    let items = (0..(PREVIEW_RENDER_LINE_LIMIT + 80))
        .map(|index| format!("dir/file-{index:03}.txt"))
        .collect::<Vec<_>>();
    let preview = container::render_iso_preview(
        container::IsoMetadata {
            volume_id: Some("BIG_IMAGE".to_string()),
            ..container::IsoMetadata::default()
        },
        container::normalize_archive_entries(items.iter().map(String::as_str), true),
    );
    let header = preview
        .header_detail(0, 20)
        .expect("iso preview header should include truncation");

    assert!(preview.truncated);
    assert!(header.contains("showing first"));
}

#[test]
fn iso_preview_lists_contents_when_bsdtar_can_read_image() {
    let root = temp_path("iso-listing");
    let image_root = root.join("image-root");
    fs::create_dir_all(image_root.join("docs")).expect("failed to create image tree");
    fs::write(image_root.join("docs/readme.txt"), "hello").expect("failed to write image file");
    let path = root.join("sample.iso");

    let created = Command::new("bsdtar")
        .arg("-cf")
        .arg(&path)
        .arg("-C")
        .arg(&image_root)
        .arg(".")
        .status();
    if !created.as_ref().is_ok_and(|status| status.success()) {
        fs::remove_dir_all(root).expect("failed to remove temp root");
        return;
    }

    let preview = build_preview(&file_entry(path));

    assert_eq!(preview.kind, PreviewKind::Archive);
    assert_eq!(preview.detail.as_deref(), Some("ISO disk image"));
    assert!(
        preview
            .lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .any(|span| span.content.contains("docs/"))
    );
    assert!(
        preview
            .lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .any(|span| span.content.contains("readme.txt"))
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn zip_preview_renders_archive_details_and_tree() {
    let root = temp_path("zip-preview");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("bundle.zip");
    write_zip_entries(
        &path,
        &[
            ("docs/readme.txt", "hello"),
            ("src/main.rs", "fn main() {}\n"),
        ],
    );

    let preview = build_preview(&file_entry(path));
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();
    let header = preview
        .header_detail(0, 20)
        .expect("zip preview should expose header detail");

    assert_eq!(preview.kind, PreviewKind::Archive);
    assert_eq!(preview.detail.as_deref(), Some("ZIP archive"));
    assert!(header.contains("ZIP archive"));
    assert!(line_texts.iter().any(|text| text.trim() == "Details"));
    assert!(!line_texts.iter().any(|text| text.trim() == "Archive"));
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Entries") && text.contains("4 total"))
    );
    assert!(line_texts.iter().any(|text| text.contains("docs/")));
    assert!(line_texts.iter().any(|text| text.contains("src/")));
    assert!(line_texts.iter().any(|text| text.contains("readme.txt")));
    assert!(line_texts.iter().any(|text| text.contains("main.rs")));

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn zip_preview_uses_license_icon_for_canonical_license_entries() {
    let root = temp_path("zip-license-icons");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("licenses.zip");
    write_zip_entries(
        &path,
        &[
            ("LICENSE", "MIT License\n"),
            ("LICENSE-MIT", "MIT License\n"),
            ("COPYING.LESSER", "GNU Lesser General Public License\n"),
            ("UNLICENSE", "The Unlicense\n"),
            ("license/readme.txt", "not a license directory\n"),
        ],
    );

    let preview = build_preview(&file_entry(path));
    let license_icon =
        theme::resolve_path_with_class(Path::new("LICENSE"), EntryKind::File, FileClass::License)
            .icon;
    let directory_icon = theme::resolve_path(Path::new("license"), EntryKind::Directory).icon;

    for name in ["LICENSE", "LICENSE-MIT", "COPYING.LESSER", "UNLICENSE"] {
        let line = preview
            .lines
            .iter()
            .find(|line| line_text(line).contains(name))
            .unwrap_or_else(|| panic!("archive preview should show {name}"));
        assert_eq!(line.spans[1].content.trim(), license_icon);
    }

    let directory_line = preview
        .lines
        .iter()
        .find(|line| line_text(line).contains("license/"))
        .expect("archive preview should show the license directory");
    assert_eq!(directory_line.spans[1].content.trim(), directory_icon);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn encrypted_seven_zip_preview_stays_responsive_with_password_notice() {
    let root = temp_path("encrypted-7z-preview");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("secret.7z");
    let mut writer = sevenz_rust2::ArchiveWriter::create(&path).expect("failed to create 7z");
    writer.set_content_methods(vec![
        sevenz_rust2::encoder_options::AesEncoderOptions::new(sevenz_rust2::Password::new(
            "secret",
        ))
        .into(),
        sevenz_rust2::encoder_options::Lzma2Options::default().into(),
    ]);
    writer
        .push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("dir/file.txt"),
            Some(&b"hello"[..]),
        )
        .expect("failed to write 7z entry");
    writer.finish().expect("failed to finish 7z");

    let preview = build_preview(&file_entry(path));
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();

    assert_eq!(preview.kind, PreviewKind::Archive);
    assert_eq!(preview.detail.as_deref(), Some("7z archive"));
    assert!(line_texts.iter().any(|text| text == "Details"));
    assert!(line_texts.iter().any(|text| text == "Contents"));
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Password-protected"))
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn corrupt_seven_zip_preview_does_not_claim_password_required() {
    let root = temp_path("corrupt-7z-preview");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("broken.7z");
    fs::write(&path, b"not a seven zip archive").expect("failed to write broken 7z");

    let preview = build_preview(&file_entry(path));
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();

    assert_eq!(preview.kind, PreviewKind::Archive);
    assert_eq!(preview.detail.as_deref(), Some("7z archive"));
    assert!(
        line_texts.iter().any(|text| text.contains("Unavailable")),
        "corrupt 7z should stay in archive preview: {line_texts:?}"
    );
    assert!(
        line_texts
            .iter()
            .all(|text| !text.contains("require a password")),
        "corrupt 7z must not be mislabeled as password-protected: {line_texts:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn corrupt_zip_preview_stays_archive_when_contents_are_unavailable() {
    let root = temp_path("corrupt-zip-preview");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("broken.zip");
    fs::write(&path, b"not a zip archive").expect("failed to write broken zip");

    let preview = build_preview(&file_entry(path));
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();

    assert_eq!(preview.kind, PreviewKind::Archive);
    assert_eq!(preview.detail.as_deref(), Some("ZIP archive"));
    assert!(line_texts.iter().any(|text| text == "Details"));
    assert!(line_texts.iter().any(|text| text == "Contents"));
    assert!(
        line_texts.iter().any(|text| text.contains("Unavailable")),
        "known archive failures should not fall through to binary preview: {line_texts:?}"
    );
    assert!(
        line_texts
            .iter()
            .all(|text| !text.contains("Binary or unsupported file")),
        "known archive failures should not render binary fallback: {line_texts:?}"
    );
    assert_eq!(preview.status_note.as_deref(), None);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn unreadable_rar_preview_stays_archive_when_contents_are_unavailable() {
    let root = temp_path("unreadable-rar-preview");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("broken.rar");
    fs::write(&path, b"not a rar archive").expect("failed to write broken rar");

    let preview = build_preview(&file_entry(path));
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();

    assert_eq!(preview.kind, PreviewKind::Archive);
    assert_eq!(preview.detail.as_deref(), Some("RAR archive"));
    assert!(
        line_texts.iter().any(|text| text.contains("Unavailable")),
        "unreadable RAR should stay in archive preview: {line_texts:?}"
    );
    assert!(
        line_texts
            .iter()
            .all(|text| !text.contains("Binary or unsupported file")),
        "unreadable RAR should not render binary fallback: {line_texts:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn rar_preview_uses_external_listing_when_available() {
    let root = temp_path("rar-preview");
    fs::create_dir_all(root.join("docs")).expect("failed to create docs dir");
    fs::create_dir_all(root.join("src")).expect("failed to create src dir");
    fs::write(root.join("docs/readme.txt"), "hello").expect("failed to write readme");
    fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("failed to write source");

    let path = root.join("bundle.rar");
    let status = Command::new("7z")
        .current_dir(&root)
        .arg("a")
        .arg("-t7z")
        .arg(&path)
        .arg("docs")
        .arg("src")
        .status();
    let Ok(status) = status else {
        fs::remove_dir_all(&root).expect("failed to remove temp root");
        return;
    };
    if !status.success() {
        fs::remove_dir_all(&root).expect("failed to remove temp root");
        return;
    }

    let preview = build_preview(&file_entry(path));
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();

    assert_eq!(preview.kind, PreviewKind::Archive);
    assert_eq!(preview.detail.as_deref(), Some("RAR archive"));
    assert!(line_texts.iter().any(|text| text == "Details"));
    assert!(line_texts.iter().any(|text| text == "Contents"));
    assert!(line_texts.iter().any(|text| text.contains("docs/")));
    assert!(line_texts.iter().any(|text| text.contains("src/")));
    assert!(line_texts.iter().any(|text| text.contains("readme.txt")));
    assert!(line_texts.iter().any(|text| text.contains("main.rs")));

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn rar_loading_preview_is_silent() {
    let root = temp_path("rar-loading-preview");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("bundle.rar");
    fs::write(&path, b"not-a-real-rar").expect("failed to write rar fixture");

    let preview = loading_preview_for(&file_entry(path), &PreviewRequestOptions::Default);

    assert_eq!(preview.kind, PreviewKind::Archive);
    assert_eq!(preview.detail.as_deref(), Some("RAR archive"));
    assert!(preview.lines.is_empty());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn comic_zip_preview_uses_comic_info_without_archive_noise() {
    let root = temp_path("comic-zip-preview");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("issue.cbz");
    let source_cover = root.join("cover.jpg");
    write_test_raster_image(&source_cover, ImageFormat::Jpeg, 160, 240);
    let cover_bytes = fs::read(&source_cover).expect("failed to read cover image");

    let file = File::create(&path).expect("failed to create comic zip");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file("001-cover.jpg", options)
        .expect("failed to start cover entry");
    zip.write_all(&cover_bytes)
        .expect("failed to write cover entry");
    zip.start_file("002-page.jpg", options)
        .expect("failed to start page entry");
    zip.write_all(&cover_bytes)
        .expect("failed to write page entry");
    zip.start_file("notes/readme.txt", options)
        .expect("failed to start text entry");
    zip.write_all(b"hello").expect("failed to write text entry");
    zip.start_file("ComicInfo.xml", options)
        .expect("failed to start comic info entry");
    zip.write_all(
        br#"<?xml version="1.0" encoding="utf-8"?>
            <ComicInfo>
              <Title>Bright Landing</Title>
              <Series>Orbital Stories</Series>
              <Number>4</Number>
              <Year>2026</Year>
              <Writer>Regueiro</Writer>
              <Publisher>Elio Press</Publisher>
              <Genre>Science Fiction</Genre>
            </ComicInfo>"#,
    )
    .expect("failed to write comic info entry");
    zip.finish().expect("failed to finish comic zip");

    let preview = build_preview(&file_entry(path));
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();
    let visual = preview
        .preview_visual
        .clone()
        .expect("comic zip should expose a page visual");

    assert_eq!(preview.kind, PreviewKind::Comic);
    assert_eq!(preview.detail.as_deref(), Some("Comic ZIP archive"));
    assert_eq!(visual.kind, PreviewVisualKind::PageImage);
    assert_eq!(visual.layout, PreviewVisualLayout::FullHeight);
    let position = preview
        .navigation_position
        .as_ref()
        .expect("comic zip should expose page navigation");
    assert_eq!(position.label, "Page");
    assert_eq!(position.index, 0);
    assert_eq!(position.count, 2);
    assert!(visual.path.exists());
    assert_eq!(line_texts.first().map(String::as_str), Some("Details"));
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Title") && text.contains("Bright Landing"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Series") && text.contains("Orbital Stories"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Number") && text.contains("4"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Writer") && text.contains("Regueiro"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Publisher") && text.contains("Elio Press"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Genre") && text.contains("Science Fiction"))
    );
    assert!(!line_texts.iter().any(|text| text.contains("Pages")));
    assert!(!line_texts.iter().any(|text| text.contains("Root")));
    assert!(!line_texts.iter().any(|text| text.trim() == "Contents"));
    assert!(!line_texts.iter().any(|text| text.contains("Extras")));
    assert!(
        !line_texts
            .iter()
            .any(|text| text.contains("Format") && text.contains("ZIP"))
    );
    assert!(!line_texts.iter().any(|text| text.contains("Packed")));
    assert!(!line_texts.iter().any(|text| text.contains("Archive Size")));
    assert!(!line_texts.iter().any(|text| text.contains("001-cover.jpg")));
    assert!(!line_texts.iter().any(|text| text.contains("002-page.jpg")));
    assert!(
        !line_texts
            .iter()
            .any(|text| text.contains("notes/readme.txt"))
    );

    let _ = fs::remove_file(visual.path);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn comic_zip_preview_reads_comic_book_info_zip_comment() {
    let root = temp_path("comic-zip-comment-metadata");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("comment.cbz");
    let file = File::create(&path).expect("failed to create comic zip");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file("001.jpg", options)
        .expect("failed to start page entry");
    zip.write_all(b"page").expect("failed to write page entry");
    zip.set_comment(
        r#"{"appID":"FixtureReader/1","ComicBookInfo/1.0":{"series":"Aurora Riders","title":"First Light","issue":"1","publisher":"Elio Press","publicationYear":1958,"genre":"Sci-Fi","credits":[{"role":"Writer","person":"Lee Maven"}]}}"#,
    )
    .expect("failed to set comic zip comment");
    zip.finish().expect("failed to finish comic zip");

    let preview = build_preview(&file_entry(path));
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();
    let visual = preview
        .preview_visual
        .clone()
        .expect("comic zip should expose a page visual");

    assert_eq!(line_texts.first().map(String::as_str), Some("Details"));
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Title") && text.contains("First Light"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Series") && text.contains("Aurora Riders"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Number") && text.contains("1"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Year") && text.contains("1958"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Publisher") && text.contains("Elio Press"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Writer") && text.contains("Lee Maven"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Genre") && text.contains("Sci-Fi"))
    );

    let _ = fs::remove_file(visual.path);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn comic_zip_preview_reads_comet_xml_metadata() {
    let root = temp_path("comic-zip-comet-metadata");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("comet.cbz");
    let file = File::create(&path).expect("failed to create comic zip");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file("001.jpg", options)
        .expect("failed to start page entry");
    zip.write_all(b"page").expect("failed to write page entry");
    zip.start_file("CoMet.xml", options)
        .expect("failed to start comet entry");
    zip.write_all(
        br#"<?xml version="1.0" encoding="utf-8"?>
            <comet>
              <title>Moon Orbit</title>
              <series>Orbital Stories</series>
              <issue>5</issue>
              <volume>2</volume>
              <publisher>Elio Press</publisher>
              <date>1994-11-05</date>
              <genre>Science Fiction</genre>
            </comet>"#,
    )
    .expect("failed to write comet entry");
    zip.finish().expect("failed to finish comic zip");

    let preview = build_preview(&file_entry(path));
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();
    let visual = preview
        .preview_visual
        .clone()
        .expect("comic zip should expose a page visual");

    assert_eq!(line_texts.first().map(String::as_str), Some("Details"));
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Title") && text.contains("Moon Orbit"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Series") && text.contains("Orbital Stories"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Number") && text.contains("5"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Volume") && text.contains("2"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Year") && text.contains("1994"))
    );

    let _ = fs::remove_file(visual.path);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn comic_zip_preview_reads_metron_info_metadata() {
    let root = temp_path("comic-zip-metron-metadata");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("metron.cbz");
    let file = File::create(&path).expect("failed to create comic zip");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file("001.jpg", options)
        .expect("failed to start page entry");
    zip.write_all(b"page").expect("failed to write page entry");
    zip.start_file("MetronInfo.xml", options)
        .expect("failed to start metron info entry");
    zip.write_all(
        br#"<?xml version="1.0" encoding="utf-8"?>
            <MetronInfo>
              <Publisher>
                <Name>Elio Press</Name>
              </Publisher>
              <Series>
                <Name>Signal Hammer</Name>
                <Volume>1</Volume>
              </Series>
              <Number>12</Number>
              <Stories>
                <Story>The End</Story>
              </Stories>
              <CoverDate>2017-06-21</CoverDate>
              <Genres>
                <Genre>Superhero</Genre>
              </Genres>
              <Credits>
                <Credit>
                  <Creator>Avery Quill</Creator>
                  <Roles>
                    <Role>Writer</Role>
                  </Roles>
                </Credit>
                <Credit>
                  <Creator>Morgan Line</Creator>
                  <Roles>
                    <Role>Artist</Role>
                  </Roles>
                </Credit>
              </Credits>
            </MetronInfo>"#,
    )
    .expect("failed to write metron info entry");
    zip.finish().expect("failed to finish comic zip");

    let preview = build_preview(&file_entry(path));
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();
    let visual = preview
        .preview_visual
        .clone()
        .expect("comic zip should expose a page visual");

    assert_eq!(line_texts.first().map(String::as_str), Some("Details"));
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Title") && text.contains("The End"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Series") && text.contains("Signal Hammer"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Number") && text.contains("12"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Volume") && text.contains("1"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Year") && text.contains("2017"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Publisher") && text.contains("Elio Press"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Writer") && text.contains("Avery Quill"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Penciller") && text.contains("Morgan Line"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Genre") && text.contains("Superhero"))
    );

    let _ = fs::remove_file(visual.path);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn comic_zip_preview_reads_acbf_metadata() {
    let root = temp_path("comic-zip-acbf-metadata");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("acbf.cbz");
    let file = File::create(&path).expect("failed to create comic zip");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file("page01.jpg", options)
        .expect("failed to start page entry");
    zip.write_all(b"page").expect("failed to write page entry");
    zip.start_file("metadata/book.acbf", options)
        .expect("failed to start acbf entry");
    zip.write_all(
        br#"<?xml version="1.0" encoding="utf-8"?>
            <ACBF xmlns="http://www.acbf.info/xml/acbf/1.1">
              <meta-data>
                <book-info>
                  <author activity="Writer">
                    <first-name>Rhea</first-name>
                    <last-name>Quinn</last-name>
                  </author>
                  <author activity="Artist">
                    <nickname>Northline Studio</nickname>
                  </author>
                  <book-title>Northline Relay</book-title>
                  <genre>Science Fiction</genre>
                  <sequence title="Futuristic Tales" volume="1">3</sequence>
                </book-info>
                <publish-info>
                  <publisher>Elio Press</publisher>
                  <publish-date value="2007-02-01">February 2007</publish-date>
                </publish-info>
              </meta-data>
            </ACBF>"#,
    )
    .expect("failed to write acbf entry");
    zip.finish().expect("failed to finish comic zip");

    let preview = build_preview(&file_entry(path));
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();
    let visual = preview
        .preview_visual
        .clone()
        .expect("comic zip should expose a page visual");

    assert_eq!(line_texts.first().map(String::as_str), Some("Details"));
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Title") && text.contains("Northline Relay"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Series") && text.contains("Futuristic Tales"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Number") && text.contains("3"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Volume") && text.contains("1"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Year") && text.contains("2007"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Publisher") && text.contains("Elio Press"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Writer") && text.contains("Rhea Quinn"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Penciller") && text.contains("Northline Studio"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Genre") && text.contains("Science Fiction"))
    );

    let _ = fs::remove_file(visual.path);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn comic_zip_preview_derives_metadata_from_structured_names() {
    let root = temp_path("comic-zip-derived-metadata");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("Moon Ledger v01 (2005) (Digital) (Fixture Group) (ED).cbz");
    write_zip_binary_entries(
        &path,
        &[
            (
                "Moon Ledger - c001 (v01) - p000 [Elio Press] [Digital] [Fixture Group].jpg",
                b"chapter-one",
            ),
            (
                "Moon Ledger - c007 (v01) - p195 [Elio Press] [Digital] [Fixture Group].png",
                b"chapter-seven",
            ),
        ],
    );

    let preview = build_preview(&file_entry(path));
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();
    let visual = preview
        .preview_visual
        .clone()
        .expect("comic zip should expose a page visual");

    assert_eq!(preview.kind, PreviewKind::Comic);
    assert_eq!(preview.detail.as_deref(), Some("Comic ZIP archive"));
    assert_eq!(line_texts.first().map(String::as_str), Some("Details"));
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Series") && text.contains("Moon Ledger"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Volume") && text.contains("1"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Year") && text.contains("2005"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Publisher") && text.contains("Elio Press"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Source") && text.contains("Digital"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Chapters") && text.contains("1-7"))
    );
    assert!(!line_texts.iter().any(|text| text.contains("Pages")));
    assert!(!line_texts.iter().any(|text| text.contains("Root")));
    assert!(!line_texts.iter().any(|text| text.trim() == "Contents"));
    assert!(!line_texts.iter().any(|text| text.contains("Extras")));
    assert!(!line_texts.iter().any(|text| text.contains("p000")));
    assert!(!line_texts.iter().any(|text| text.contains("Fixture Group")));
    assert!(!line_texts.iter().any(|text| text.contains("ED")));

    let _ = fs::remove_file(visual.path);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn comic_zip_preview_derives_tome_volume_and_digital_source_from_filenames() {
    let root = temp_path("comic-zip-tome-derived-metadata");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("Skyline Saga T01 (Mira) (2018) [Digital-1699] [Fixture FR].cbz");
    write_zip_binary_entries(
        &path,
        &[("0001_0000.jpg", b"cover"), ("0002_0001.jpg", b"page")],
    );

    let preview = build_preview(&file_entry(path));
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();
    let visual = preview
        .preview_visual
        .clone()
        .expect("comic zip should expose a page visual");

    assert_eq!(line_texts.first().map(String::as_str), Some("Details"));
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Series") && text.contains("Skyline Saga"))
    );
    assert!(
        !line_texts
            .iter()
            .any(|text| text.contains("Series") && text.contains("T01"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Volume") && text.contains("1"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Year") && text.contains("2018"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Source") && text.contains("Digital"))
    );

    let _ = fs::remove_file(visual.path);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn comic_zip_preview_derives_metadata_from_bracketed_names_without_group_noise() {
    let root = temp_path("comic-zip-bracketed-derived-metadata");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("[ScanGroup] Harbor Case v01 [Digital].cbz");
    write_zip_binary_entries(&path, &[("001.jpg", b"cover"), ("002.jpg", b"page-one")]);

    let preview = build_preview(&file_entry(path));
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();
    let visual = preview
        .preview_visual
        .clone()
        .expect("comic zip should expose a page visual");

    assert_eq!(line_texts.first().map(String::as_str), Some("Details"));
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Series") && text.contains("Harbor Case"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Volume") && text.contains("1"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Source") && text.contains("Digital"))
    );
    assert!(!line_texts.iter().any(|text| text.contains("ScanGroup")));

    let _ = fs::remove_file(visual.path);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn comic_zip_preview_derives_series_from_collection_folder() {
    let root = temp_path("comic-zip-folder-metadata");
    let collection = root.join("[FixtureGroup] Harbor Case");
    fs::create_dir_all(&collection).expect("failed to create collection folder");
    let path = collection.join("Volume 01.cbz");
    write_zip_binary_entries(&path, &[("000.jpg", b"cover"), ("001.png", b"page-one")]);

    let preview = build_preview(&file_entry(path));
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();
    let visual = preview
        .preview_visual
        .clone()
        .expect("comic zip should expose a page visual");

    assert_eq!(preview.kind, PreviewKind::Comic);
    assert_eq!(preview.detail.as_deref(), Some("Comic ZIP archive"));
    assert_eq!(line_texts.first().map(String::as_str), Some("Details"));
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Series") && text.contains("Harbor Case"))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Volume") && text.contains("1"))
    );
    assert!(!line_texts.iter().any(|text| text.contains("FixtureGroup")));
    assert!(!line_texts.iter().any(|text| text.contains("Pages")));
    assert!(!line_texts.iter().any(|text| text.contains("Root")));
    assert!(!line_texts.iter().any(|text| text.contains("000.jpg")));

    let _ = fs::remove_file(visual.path);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn comic_zip_preview_avoids_generic_derived_metadata() {
    let root = temp_path("comic-zip-generic-derived-metadata");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("issue.cbz");
    write_zip_binary_entries(
        &path,
        &[
            ("c001-p001 [Digital].jpg", b"cover"),
            ("c001-p002.jpg", b"page-one"),
        ],
    );

    let preview = build_preview(&file_entry(path));
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();
    let visual = preview
        .preview_visual
        .clone()
        .expect("comic zip should expose a page visual");

    assert!(line_texts.is_empty());

    let _ = fs::remove_file(visual.path);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn comic_zip_preview_uses_natural_page_order_and_page_selection() {
    let root = temp_path("comic-zip-pages");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("issue.cbz");
    write_zip_binary_entries(
        &path,
        &[
            ("18376941278364981273/10.jpg", b"page-ten"),
            ("18376941278364981273/2.jpg", b"page-two"),
            ("18376941278364981273/1.jpg", b"page-one"),
        ],
    );

    let first_preview = build_preview(&file_entry(path.clone()));
    let first_visual = first_preview
        .preview_visual
        .as_ref()
        .expect("first page should be extracted");
    let second_preview = build_preview_with_options(
        &file_entry(path.clone()),
        &PreviewRequestOptions::ComicPage(1),
    );
    let second_visual = second_preview
        .preview_visual
        .as_ref()
        .expect("second page should be extracted");
    let third_preview = build_preview_with_options(
        &file_entry(path.clone()),
        &PreviewRequestOptions::ComicPage(2),
    );
    let third_visual = third_preview
        .preview_visual
        .as_ref()
        .expect("third page should be extracted");

    assert_eq!(
        fs::read(&first_visual.path).expect("failed to read first page"),
        b"page-one"
    );
    assert_eq!(
        fs::read(&second_visual.path).expect("failed to read second page"),
        b"page-two"
    );
    assert_eq!(
        fs::read(&third_visual.path).expect("failed to read third page"),
        b"page-ten"
    );
    assert_eq!(
        second_preview
            .navigation_position
            .as_ref()
            .map(|position| position.index),
        Some(1)
    );
    assert_eq!(
        third_preview
            .navigation_position
            .as_ref()
            .map(|position| position.count),
        Some(3)
    );
    let second_line_texts: Vec<_> = second_preview.lines.iter().map(line_text).collect();
    assert!(second_line_texts.is_empty());
    assert!(!second_line_texts.iter().any(|text| text.contains("2.jpg")));
    assert!(
        !second_line_texts
            .iter()
            .any(|text| text.contains("18376941278364981273"))
    );

    let _ = fs::remove_file(first_visual.path.clone());
    let _ = fs::remove_file(second_visual.path.clone());
    let _ = fs::remove_file(third_visual.path.clone());
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn cbr_file_with_zip_content_renders_as_comic_preview() {
    let root = temp_path("comic-rar-preview");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("issue.cbr");
    let source_cover = root.join("cover.jpg");
    write_test_raster_image(&source_cover, ImageFormat::Jpeg, 160, 240);
    let cover_bytes = fs::read(&source_cover).expect("failed to read cover image");
    write_zip_binary_entries(
        &path,
        &[
            ("001-cover.jpg", &cover_bytes),
            ("002-page.jpg", &cover_bytes),
        ],
    );

    let preview = build_preview(&file_entry(path));
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();
    let visual = preview
        .preview_visual
        .clone()
        .expect("comic rar should expose a page visual");

    assert_eq!(preview.kind, PreviewKind::Comic);
    assert_eq!(preview.detail.as_deref(), Some("Comic RAR archive"));
    assert_eq!(visual.kind, PreviewVisualKind::PageImage);
    assert_eq!(visual.layout, PreviewVisualLayout::FullHeight);
    assert_eq!(
        preview.navigation_position.as_ref().map(|position| (
            position.label,
            position.index,
            position.count
        )),
        Some(("Page", 0, 2))
    );
    assert!(visual.path.exists());
    assert!(line_texts.is_empty());

    let _ = fs::remove_file(visual.path);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn tar_preview_lists_inner_archive_contents() {
    let root = temp_path("tar-preview");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("bundle.tar");
    write_tar_entries(
        &path,
        &[
            ("docs/readme.txt", "hello"),
            ("src/main.rs", "fn main() {}\n"),
        ],
    );

    let preview = build_preview(&file_entry(path));
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();

    assert_eq!(preview.kind, PreviewKind::Archive);
    assert_eq!(preview.detail.as_deref(), Some("TAR archive"));
    assert!(line_texts.iter().any(|text| text.contains("docs/")));
    assert!(line_texts.iter().any(|text| text.contains("src/")));
    assert!(line_texts.iter().any(|text| text.contains("readme.txt")));
    assert!(line_texts.iter().any(|text| text.contains("main.rs")));
    assert!(!line_texts.iter().any(|text| text.contains("bundle.tar")));

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn tar_gz_preview_lists_inner_archive_contents() {
    let root = temp_path("tar-gz-preview");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("bundle.tar.gz");
    write_tar_gz_entries(
        &path,
        &[
            ("docs/readme.txt", "hello"),
            ("src/main.rs", "fn main() {}\n"),
        ],
    );

    let preview = build_preview(&file_entry(path));
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();

    assert_eq!(preview.kind, PreviewKind::Archive);
    assert_eq!(preview.detail.as_deref(), Some("TAR.GZ archive"));
    assert!(line_texts.iter().any(|text| text.contains("docs/")));
    assert!(line_texts.iter().any(|text| text.contains("src/")));
    assert!(line_texts.iter().any(|text| text.contains("readme.txt")));
    assert!(line_texts.iter().any(|text| text.contains("main.rs")));
    assert!(!line_texts.iter().any(|text| text.contains("bundle.tar")));

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn tgz_preview_keeps_tar_gz_label_and_contents_tree() {
    let root = temp_path("tgz-preview");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("bundle.tgz");
    write_tar_gz_entries(
        &path,
        &[("assets/logo.txt", "logo"), ("bin/elio", "#!/bin/sh\n")],
    );

    let preview = build_preview(&file_entry(path));
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();

    assert_eq!(preview.kind, PreviewKind::Archive);
    assert_eq!(preview.detail.as_deref(), Some("TAR.GZ archive"));
    assert!(line_texts.iter().any(|text| text.contains("assets/")));
    assert!(line_texts.iter().any(|text| text.contains("bin/")));
    assert!(line_texts.iter().any(|text| text.contains("logo.txt")));
    assert!(line_texts.iter().any(|text| text.contains("elio")));
    assert!(!line_texts.iter().any(|text| text.contains("bundle.tar")));

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn raw_xz_preview_uses_compressed_disk_image_label() {
    let root = temp_path("raw-xz-preview");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let path = root.join("fedora.aarch64.raw.xz");
    if !write_xz_compressed_file(&path, b"raw-disk-image") {
        fs::remove_dir_all(root).expect("failed to remove temp root");
        return;
    }

    let preview = build_preview(&file_entry(path));
    let line_texts: Vec<_> = preview.lines.iter().map(line_text).collect();

    // On systems without 7z or bsdtar support for raw XZ images, the preview
    // falls back to Binary. Skip the remaining assertions in that case.
    if preview.kind == PreviewKind::Binary {
        fs::remove_dir_all(root).expect("failed to remove temp root");
        return;
    }

    assert_eq!(preview.kind, PreviewKind::Archive);
    assert_eq!(
        preview.detail.as_deref(),
        Some("XZ-compressed raw disk image")
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("Format") && (text.contains("XZ") || text.contains("xz")))
    );
    assert!(
        line_texts
            .iter()
            .any(|text| text.contains("fedora.aarch64.raw"))
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}
