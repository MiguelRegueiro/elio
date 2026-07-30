mod discovery;
mod input;
mod overlay;
#[cfg(test)]
mod tests;

#[cfg(test)]
use std::cell::RefCell;
use std::path::Path;

#[cfg(any(test, all(unix, not(target_os = "macos"))))]
use super::state::OpenWithApp;
#[cfg(all(unix, not(target_os = "macos")))]
use crate::core::Entry;
use crate::{
    core::{EntryKind, FileClass},
    file_info::{PreviewKind, inspect_path},
};

#[cfg(all(test, unix, not(target_os = "macos")))]
thread_local! {
    static TEST_DEFAULT_OPEN_WITH_APP: RefCell<Option<OpenWithApp>> = const { RefCell::new(None) };
    static TEST_OPEN_WITH_APPS_FOUND: RefCell<Option<bool>> = const { RefCell::new(None) };
    static TEST_EDITOR_FALLBACK_APP: RefCell<Option<OpenWithApp>> = const { RefCell::new(None) };
}

#[cfg(test)]
thread_local! {
    static TEST_DISCOVER_OPEN_WITH_APPS: RefCell<Option<Vec<OpenWithApp>>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub(in crate::app) fn set_discovered_open_with_apps_for_test(apps: Option<Vec<OpenWithApp>>) {
    TEST_DISCOVER_OPEN_WITH_APPS.with(|slot| *slot.borrow_mut() = apps);
}

#[cfg(test)]
pub(in crate::app) fn discovered_open_with_apps_for_test() -> Option<Vec<OpenWithApp>> {
    TEST_DISCOVER_OPEN_WITH_APPS.with(|slot| slot.borrow().clone())
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(in crate::app) fn default_open_with_app_for_entry(entry: &Entry) -> Option<OpenWithApp> {
    #[cfg(test)]
    {
        let _ = entry;
        TEST_DEFAULT_OPEN_WITH_APP.with(|slot| slot.borrow().clone())
    }

    #[cfg(not(test))]
    discovery::discover_desktop_apps_for_entry(entry)
        .into_iter()
        .find(|app| app.is_default)
}

#[cfg(all(test, unix, not(target_os = "macos")))]
pub(in crate::app) fn set_default_open_with_app_for_test(app: Option<OpenWithApp>) {
    TEST_DEFAULT_OPEN_WITH_APP.with(|slot| *slot.borrow_mut() = app);
}

#[cfg(all(test, unix, not(target_os = "macos")))]
pub(in crate::app) fn set_open_with_apps_found_for_test(found: Option<bool>) {
    TEST_OPEN_WITH_APPS_FOUND.with(|slot| *slot.borrow_mut() = found);
}

#[cfg(all(test, unix, not(target_os = "macos")))]
pub(in crate::app) fn set_editor_fallback_app_for_test(app: Option<OpenWithApp>) {
    TEST_EDITOR_FALLBACK_APP.with(|slot| *slot.borrow_mut() = app);
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(in crate::app) fn open_with_apps_found_for_entry(entry: &Entry) -> bool {
    #[cfg(test)]
    {
        let _ = entry;
        TEST_OPEN_WITH_APPS_FOUND.with(|slot| slot.borrow().unwrap_or(true))
    }

    #[cfg(not(test))]
    {
        !discovery::discover_desktop_apps_for_entry(entry).is_empty()
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(in crate::app) fn editor_fallback_app_for_entry(entry: &Entry) -> Option<OpenWithApp> {
    #[cfg(test)]
    {
        let _ = entry;
        TEST_EDITOR_FALLBACK_APP.with(|slot| slot.borrow().clone())
    }

    #[cfg(not(test))]
    {
        discovery::editor_fallback_app_for_entry(entry)
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(super) fn path_is_text_like(path: &Path) -> bool {
    let facts = inspect_path(path, EntryKind::File);

    match facts.preview.kind {
        PreviewKind::Markdown | PreviewKind::Csv => true,
        // Source previews are usually a good editor fit, but image formats like
        // SVG should still behave like images in "Open With".
        PreviewKind::Source => facts.builtin_class != FileClass::Image,
        // Plain-text previews cover both true text files and some binary
        // document/image categories that render metadata as text. Only treat
        // them as editor-friendly when they are not one of those richer types.
        PreviewKind::PlainText => {
            facts.preview.document_format.is_none()
                && !matches!(
                    facts.builtin_class,
                    FileClass::Image
                        | FileClass::Audio
                        | FileClass::Video
                        | FileClass::Archive
                        | FileClass::Font
                )
        }
        _ => false,
    }
}
