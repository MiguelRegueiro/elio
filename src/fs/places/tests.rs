#[cfg(all(unix, not(any(target_os = "macos", target_os = "ios"))))]
use super::resolution::parse_user_dir;
use super::resolution::{
    PlaceResolutionContext, build_sidebar_rows_with_context, resolve_personal_dir,
};
use crate::{
    config::{BuiltinPlace, PlaceEntrySpec, PlacesConfig},
    core::{SidebarItemKind, SidebarRow},
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

fn temp_path(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("elio-places-{label}-{unique}"))
}

fn context_for(root: &Path) -> PlaceResolutionContext {
    let home = root.join("home");
    let downloads = home.join("Downloads");
    let trash = root.join("trash");
    fs::create_dir_all(&downloads).expect("failed to create downloads");
    fs::create_dir_all(&trash).expect("failed to create trash");
    PlaceResolutionContext {
        home,
        desktop: None,
        documents: None,
        downloads: Some(downloads),
        pictures: None,
        music: None,
        videos: None,
        root: None,
        trash: Some(trash),
    }
}

#[test]
fn personal_dir_uses_current_resolution_for_normal_sessions() {
    let home = temp_path("normal-home");
    let localized = home.join("Descargas");
    fs::create_dir_all(&localized).expect("failed to create localized place");

    assert_eq!(
        resolve_personal_dir(
            &home,
            Some(&home),
            Some(localized.clone()),
            None,
            Some("Downloads"),
        ),
        Some(localized)
    );

    fs::remove_dir_all(home).expect("failed to remove temp home");
}

#[test]
fn personal_dir_uses_invoking_home_when_process_home_differs() {
    let root = temp_path("elevated-home");
    let home = root.join("paco");
    let downloads = home.join("Downloads");
    fs::create_dir_all(&downloads).expect("failed to create invoking-user downloads");

    assert_eq!(
        resolve_personal_dir(
            &home,
            Some(Path::new("/root")),
            Some(PathBuf::from("/root/Downloads")),
            None,
            Some("Downloads"),
        ),
        Some(downloads)
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn personal_dir_prefers_invoking_users_configured_directory() {
    let root = temp_path("configured-home");
    let home = root.join("paco");
    let downloads = home.join("Descargas");
    fs::create_dir_all(&downloads).expect("failed to create configured downloads");

    assert_eq!(
        resolve_personal_dir(
            &home,
            Some(Path::new("/root")),
            Some(PathBuf::from("/root/Downloads")),
            Some(downloads.clone()),
            Some("Downloads"),
        ),
        Some(downloads)
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn personal_dir_does_not_invent_missing_elevated_unix_place() {
    let root = temp_path("missing-place");
    let home = root.join("paco");
    let downloads = home.join("Downloads");
    fs::create_dir_all(&downloads).expect("failed to create conventional downloads");

    assert_eq!(
        resolve_personal_dir(
            &home,
            Some(Path::new("/root")),
            Some(PathBuf::from("/root/Downloads")),
            None,
            None,
        ),
        None
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "ios"))))]
#[test]
fn user_dir_parser_handles_home_relative_absolute_and_escaped_paths() {
    let home = Path::new("/home/paco");
    assert_eq!(
        parse_user_dir(b"XDG_DOWNLOAD_DIR=\"$HOME/Descargas\"", home),
        Some(("DOWNLOAD".to_string(), home.join("Descargas")))
    );
    assert_eq!(
        parse_user_dir(b"XDG_DOCUMENTS_DIR=\"/srv/paco/docs\"", home),
        Some(("DOCUMENTS".to_string(), PathBuf::from("/srv/paco/docs")))
    );
    assert_eq!(
        parse_user_dir(b"XDG_MUSIC_DIR=\"$HOME/My\\ Music\"", home),
        Some(("MUSIC".to_string(), home.join("My Music")))
    );
    assert_eq!(
        parse_user_dir(b"XDG_PICTURES_DIR=\"$HOME/My\\\" Photos\" # comment", home),
        Some(("PICTURES".to_string(), home.join("My\" Photos")))
    );
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "ios"))))]
#[test]
fn user_dir_parser_rejects_disabled_relative_and_malformed_paths() {
    let home = Path::new("/home/paco");
    assert_eq!(parse_user_dir(b"XDG_DOWNLOAD_DIR=\"$HOME/\"", home), None);
    assert_eq!(
        parse_user_dir(b"XDG_DOWNLOAD_DIR=\"Downloads\"", home),
        None
    );
    assert_eq!(
        parse_user_dir(b"XDG_DOWNLOAD_DIR=$HOME/Downloads", home),
        None
    );
    assert_eq!(
        parse_user_dir(b"XDG_DOWNLOAD_DIR=\"$HOME/broken\\\"", home),
        None
    );
}

#[test]
fn configured_places_order_and_semantic_kinds_are_preserved() {
    let root = temp_path("ordered-sidebar");
    let context = context_for(&root);
    let projects = root.join("projects");
    let places = PlacesConfig {
        show_devices: false,
        entries: vec![
            PlaceEntrySpec::Builtin {
                place: BuiltinPlace::Downloads,
                icon: Some("D".to_string()),
            },
            PlaceEntrySpec::Custom {
                title: "Projects".to_string(),
                path: projects.clone(),
                icon: Some("P".to_string()),
            },
            PlaceEntrySpec::Builtin {
                place: BuiltinPlace::Home,
                icon: None,
            },
            PlaceEntrySpec::Builtin {
                place: BuiltinPlace::Trash,
                icon: None,
            },
        ],
    };

    let rows = build_sidebar_rows_with_context(&places, &context);
    let items = rows.iter().filter_map(SidebarRow::item).collect::<Vec<_>>();

    assert_eq!(items.len(), 4);
    assert_eq!(items[0].title, "Downloads");
    assert_eq!(items[0].kind, SidebarItemKind::Downloads);
    assert_eq!(items[0].icon, "D");
    assert_eq!(items[1].title, "Projects");
    assert_eq!(items[1].kind, SidebarItemKind::Custom);
    assert_eq!(items[1].icon, "P");
    assert_eq!(items[1].path, projects);
    assert_eq!(items[2].title, "Home");
    assert_eq!(items[2].kind, SidebarItemKind::Home);
    assert_eq!(items[3].title, "Trash");
    assert_eq!(items[3].kind, SidebarItemKind::Trash);
    assert!(rows.iter().all(|row| matches!(row, SidebarRow::Item(_))));

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn missing_builtin_places_are_skipped_but_nonexistent_custom_places_stay_visible() {
    let root = temp_path("missing-builtins");
    let context = context_for(&root);
    let future_mount = root.join("mnt").join("camera");
    let places = PlacesConfig {
        show_devices: false,
        entries: vec![
            PlaceEntrySpec::Builtin {
                place: BuiltinPlace::Desktop,
                icon: None,
            },
            PlaceEntrySpec::Custom {
                title: "Camera".to_string(),
                path: future_mount.clone(),
                icon: None,
            },
            PlaceEntrySpec::Builtin {
                place: BuiltinPlace::Downloads,
                icon: None,
            },
        ],
    };

    let rows = build_sidebar_rows_with_context(&places, &context);
    let items = rows.iter().filter_map(SidebarRow::item).collect::<Vec<_>>();

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].title, "Camera");
    assert_eq!(items[0].kind, SidebarItemKind::Custom);
    assert_eq!(items[0].path, future_mount);
    assert_eq!(items[1].title, "Downloads");

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn localized_builtin_places_show_resolved_folder_name() {
    let root = temp_path("localized-builtins");
    let home = root.join("home");
    let downloads = home.join("Descargas");
    fs::create_dir_all(&downloads).expect("failed to create downloads");
    let context = PlaceResolutionContext {
        home,
        desktop: None,
        documents: None,
        downloads: Some(downloads.clone()),
        pictures: None,
        music: None,
        videos: None,
        root: None,
        trash: None,
    };
    let places = PlacesConfig {
        show_devices: false,
        entries: vec![PlaceEntrySpec::Builtin {
            place: BuiltinPlace::Downloads,
            icon: None,
        }],
    };

    let rows = build_sidebar_rows_with_context(&places, &context);
    let items = rows.iter().filter_map(SidebarRow::item).collect::<Vec<_>>();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Descargas");
    assert_eq!(items[0].kind, SidebarItemKind::Downloads);
    assert_eq!(items[0].path, downloads);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn places_deduplicate_entries_by_resolved_path() {
    let root = temp_path("dedupe-sidebar");
    let context = context_for(&root);
    let places = PlacesConfig {
        show_devices: false,
        entries: vec![
            PlaceEntrySpec::Builtin {
                place: BuiltinPlace::Home,
                icon: None,
            },
            PlaceEntrySpec::Custom {
                title: "Home 2".to_string(),
                path: context.home.clone(),
                icon: Some("H".to_string()),
            },
            PlaceEntrySpec::Builtin {
                place: BuiltinPlace::Downloads,
                icon: None,
            },
            PlaceEntrySpec::Custom {
                title: "Downloads Alias".to_string(),
                path: context.home.join("Downloads").join("..").join("Downloads"),
                icon: Some("A".to_string()),
            },
        ],
    };

    let rows = build_sidebar_rows_with_context(&places, &context);
    let items = rows.iter().filter_map(SidebarRow::item).collect::<Vec<_>>();

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].title, "Home");
    assert_eq!(items[1].title, "Downloads");

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[cfg(unix)]
#[test]
fn custom_symlinked_places_store_resolved_identity_path() {
    use std::os::unix::fs::symlink;

    let root = temp_path("symlink-identity-sidebar");
    let context = context_for(&root);
    let target = root.join("target");
    let linked = root.join("linked");
    fs::create_dir_all(&target).expect("failed to create target dir");
    symlink(&target, &linked).expect("failed to create symlinked place");
    let places = PlacesConfig {
        show_devices: false,
        entries: vec![PlaceEntrySpec::Custom {
            title: "Linked".to_string(),
            path: linked.clone(),
            icon: None,
        }],
    };

    let rows = build_sidebar_rows_with_context(&places, &context);
    let items = rows.iter().filter_map(SidebarRow::item).collect::<Vec<_>>();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].path, linked);
    assert_eq!(
        items[0].identity_path,
        target.canonicalize().expect("target should canonicalize")
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[cfg(unix)]
#[test]
fn symlinked_places_use_link_icon_unless_icon_is_configured() {
    use std::os::unix::fs::symlink;

    let root = temp_path("symlink-icons-sidebar");
    let home = root.join("home");
    let downloads = home.join("Downloads");
    let downloads_target = root.join("downloads-target");
    let normal = root.join("normal");
    let linked = root.join("linked");
    let linked_target = root.join("linked-target");
    let manual = root.join("manual");
    let manual_target = root.join("manual-target");
    let broken = root.join("broken");
    let missing_target = root.join("missing-target");
    for dir in [
        &home,
        &downloads_target,
        &normal,
        &linked_target,
        &manual_target,
    ] {
        fs::create_dir_all(dir).expect("failed to create test dir");
    }
    for (target, link) in [
        (&downloads_target, &downloads),
        (&linked_target, &linked),
        (&manual_target, &manual),
        (&missing_target, &broken),
    ] {
        symlink(target, link).expect("failed to create symlinked place");
    }
    let context = PlaceResolutionContext {
        home,
        desktop: None,
        documents: None,
        downloads: Some(downloads),
        pictures: None,
        music: None,
        videos: None,
        root: None,
        trash: None,
    };
    let places = PlacesConfig {
        show_devices: false,
        entries: vec![
            PlaceEntrySpec::Builtin {
                place: BuiltinPlace::Downloads,
                icon: None,
            },
            PlaceEntrySpec::Custom {
                title: "Normal".to_string(),
                path: normal,
                icon: None,
            },
            PlaceEntrySpec::Custom {
                title: "Linked".to_string(),
                path: linked,
                icon: None,
            },
            PlaceEntrySpec::Custom {
                title: "Manual".to_string(),
                path: manual,
                icon: Some("L".to_string()),
            },
            PlaceEntrySpec::Custom {
                title: "Broken".to_string(),
                path: broken,
                icon: None,
            },
        ],
    };

    let rows = build_sidebar_rows_with_context(&places, &context);
    let items = rows.iter().filter_map(SidebarRow::item).collect::<Vec<_>>();

    assert_eq!(items.len(), 5);
    assert_eq!(items[0].title, "Downloads");
    assert_eq!(items[0].icon, "");
    assert_eq!(items[1].title, "Normal");
    assert_eq!(items[1].icon, "󰉋");
    assert_eq!(items[2].title, "Linked");
    assert_eq!(items[2].icon, "");
    assert_eq!(items[3].title, "Manual");
    assert_eq!(items[3].icon, "L");
    assert_eq!(items[4].title, "Broken");
    assert_eq!(items[4].icon, "󰌺");

    fs::remove_dir_all(root).expect("failed to remove temp root");
}
