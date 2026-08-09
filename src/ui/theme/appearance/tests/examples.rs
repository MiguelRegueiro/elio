use super::super::rules::rgb;
use super::*;
use ratatui::style::Color;

const ALTERNATE_EXAMPLE_THEME_NAMES: &[&str] = &[
    "default-light",
    "blush-light",
    "amber-dusk",
    "catppuccin-mocha",
    "tokyo-night",
    "nord",
    "navi",
    "neon-cherry",
    "terminal-ansi",
    "transparent",
];

const GENERIC_DEV_DIRECTORIES: &[&str] = &[
    "node_modules",
    "tests",
    "test",
    "__tests__",
    "scripts",
    "build",
    "dist",
    ".next",
    ".nuxt",
    ".svelte-kit",
    ".astro",
    "assets",
    "coverage",
    "tmp",
    "temp",
    "out",
    "target",
    "bin",
    "lib",
    "vendor",
    "src",
    "config",
    "docs",
];

fn alternate_example_theme_config(name: &str) -> &'static str {
    match name {
        "default-light" => include_str!("../../../../../examples/themes/default-light/theme.toml"),
        "blush-light" => include_str!("../../../../../examples/themes/blush-light/theme.toml"),
        "amber-dusk" => include_str!("../../../../../examples/themes/amber-dusk/theme.toml"),
        "catppuccin-mocha" => {
            include_str!("../../../../../examples/themes/catppuccin-mocha/theme.toml")
        }
        "tokyo-night" => include_str!("../../../../../examples/themes/tokyo-night/theme.toml"),
        "nord" => include_str!("../../../../../examples/themes/nord/theme.toml"),
        "navi" => include_str!("../../../../../examples/themes/navi/theme.toml"),
        "neon-cherry" => include_str!("../../../../../examples/themes/neon-cherry/theme.toml"),
        "terminal-ansi" => include_str!("../../../../../examples/themes/terminal-ansi/theme.toml"),
        "transparent" => include_str!("../../../../../examples/themes/transparent/theme.toml"),
        _ => panic!("unknown alternate example theme fixture: {name}"),
    }
}

fn load_alternate_example_theme(name: &str) -> Theme {
    Theme::from_config_str(alternate_example_theme_config(name)).unwrap_or_else(|error| {
        panic!("{name} example theme should parse as a user theme: {error}")
    })
}

fn assert_symlink_directory_matches_folder_color(theme: &Theme, label: &str) {
    let directory = theme.classes.get(&FileClass::Directory).unwrap();
    let symlink_directory = theme.classes.get(&FileClass::SymlinkDirectory).unwrap();

    assert_eq!(
        symlink_directory.icon, "",
        "{label}: symlinked directories should use the linked-folder icon",
    );
    assert_eq!(
        symlink_directory.color, directory.color,
        "{label}: symlinked directories should use the normal folder color",
    );
}

fn assert_uses_normal_folder_color_for_generic_dev_directories(theme: &Theme, label: &str) {
    let normal_folder_color = theme
        .resolve(Path::new("projects"), EntryKind::Directory)
        .color;

    for directory in GENERIC_DEV_DIRECTORIES {
        let resolved = theme.resolve(Path::new(directory), EntryKind::Directory);
        assert_eq!(
            resolved.class,
            FileClass::Directory,
            "{label}: {directory} should resolve as a directory",
        );
        assert_eq!(
            resolved.color, normal_folder_color,
            "{label}: {directory} should use the normal folder color",
        );
    }
}

#[test]
fn alternate_example_themes_style_symlinks_like_their_base_kinds() {
    for name in ALTERNATE_EXAMPLE_THEME_NAMES {
        let theme = load_alternate_example_theme(name);
        assert_symlink_directory_matches_folder_color(&theme, name);
    }
}

#[test]
fn alternate_example_themes_use_theme_native_subtitle_colors() {
    for (name, expected_color) in [
        ("default-light", rgb(0x71, 0x68, 0xd9)),
        ("blush-light", rgb(0xa0, 0x8d, 0xd1)),
        ("amber-dusk", rgb(0xaf, 0x89, 0xbf)),
        ("catppuccin-mocha", rgb(0xb4, 0xbe, 0xfe)),
        ("tokyo-night", rgb(0x9d, 0x7c, 0xd8)),
        ("terminal-ansi", Color::Magenta),
    ] {
        let theme = load_alternate_example_theme(name);

        for path in [
            "movie.srt",
            "movie.vtt",
            "movie.ass",
            "movie.ssa",
            "movie.ttml",
            "movie.sbv",
            "movie.smi",
        ] {
            let subtitles = theme.resolve(Path::new(path), EntryKind::File);

            assert_eq!(subtitles.class, FileClass::Document, "{name}: {path}");
            assert_eq!(subtitles.icon, "󰨖", "{name}: {path}");
            assert_eq!(subtitles.color, expected_color, "{name}: {path}");
        }
    }
}

#[test]
fn blush_light_example_theme_parses_as_user_theme_and_applies_custom_icon_and_code_colors() {
    let theme = load_alternate_example_theme("blush-light");

    assert_eq!(theme.preview.code.keyword, rgb(0xd8, 0x63, 0x92));
    assert_eq!(theme.preview.code.function, rgb(0x8f, 0x71, 0xbf));

    let directory = theme.resolve(Path::new("projects"), EntryKind::Directory);
    assert_eq!(directory.class, FileClass::Directory);
    assert_eq!(directory.icon, "󰉋");
    assert_eq!(directory.color, rgb(0xd4, 0x6b, 0x93));

    let rust = theme.resolve(Path::new("main.rs"), EntryKind::File);
    assert_eq!(rust.class, FileClass::Code);
    assert_eq!(rust.icon, "");
    assert_eq!(rust.color, rgb(0xca, 0x81, 0x68));

    let readme = theme.resolve(Path::new("README.md"), EntryKind::File);
    assert_eq!(readme.class, FileClass::Document);
    assert_eq!(readme.icon, "");
    assert_eq!(readme.color, rgb(0xbb, 0x90, 0x7b));
}

#[test]
fn default_light_example_theme_parses_as_user_theme_and_preserves_default_icon_mappings() {
    let theme = load_alternate_example_theme("default-light");

    assert_eq!(theme.palette.bg, rgb(0xf5, 0xf9, 0xff));
    assert_eq!(theme.preview.code.keyword, rgb(0x7a, 0xae, 0xff));
    assert_eq!(theme.preview.code.function, rgb(0x46, 0x9f, 0xc3));
    assert_eq!(theme.preview.code.string, rgb(0x4d, 0x92, 0x79));
    assert_eq!(theme.preview.code.r#type, rgb(0x8a, 0x74, 0xc8));

    let directory = theme.resolve(Path::new("projects"), EntryKind::Directory);
    assert_eq!(directory.class, FileClass::Directory);
    assert_eq!(directory.icon, "󰉋");
    assert_eq!(directory.color, rgb(0x5b, 0xa8, 0xff));

    let downloads = theme.resolve(Path::new("Downloads"), EntryKind::Directory);
    assert_eq!(downloads.class, FileClass::Directory);
    assert_eq!(downloads.icon, "󰉍");
    assert_eq!(downloads.color, rgb(0xb9, 0x97, 0x3e));

    let pictures = theme.resolve(Path::new("Pictures"), EntryKind::Directory);
    assert_eq!(pictures.class, FileClass::Directory);
    assert_eq!(pictures.icon, "󰉏");
    assert_eq!(pictures.color, rgb(0x55, 0xa7, 0x9e));

    let music = theme.resolve(Path::new("Music"), EntryKind::Directory);
    assert_eq!(music.class, FileClass::Directory);
    assert_eq!(music.icon, "󱍙");
    assert_eq!(music.color, rgb(0x9a, 0x81, 0xcf));

    let src = theme.resolve(Path::new("src"), EntryKind::Directory);
    assert_eq!(src.class, FileClass::Directory);
    assert_eq!(src.icon, "󰉋");
    assert_eq!(src.color, rgb(0x5b, 0xa8, 0xff));

    let shell = theme.resolve(Path::new("deploy.sh"), EntryKind::File);
    assert_eq!(shell.class, FileClass::Code);
    assert_eq!(shell.icon, "");
    assert_eq!(shell.color, rgb(0x69, 0x78, 0x8b));

    let rust = theme.resolve(Path::new("main.rs"), EntryKind::File);
    assert_eq!(rust.class, FileClass::Code);
    assert_eq!(rust.icon, "");
    assert_eq!(rust.color, rgb(0xb8, 0x74, 0x45));

    let package = theme.resolve(Path::new("package.json"), EntryKind::File);
    assert_eq!(package.class, FileClass::Config);
    assert_eq!(package.icon, "󰏗");
    assert_eq!(package.color, rgb(0x7d, 0xb0, 0xff));

    let readme = theme.resolve(Path::new("README.md"), EntryKind::File);
    assert_eq!(readme.class, FileClass::Document);
    assert_eq!(readme.icon, "");
    assert_eq!(readme.color, rgb(0xab, 0x97, 0x7a));

    let turbo = theme.resolve(Path::new("turbo.json"), EntryKind::File);
    assert_eq!(turbo.class, FileClass::Config);
    assert_eq!(turbo.icon, "󰐷");
    assert_eq!(turbo.color, rgb(0x72, 0x81, 0x95));
}

#[test]
fn amber_dusk_example_theme_parses_as_user_theme_and_applies_warm_dark_palette() {
    let theme = load_alternate_example_theme("amber-dusk");

    assert_eq!(theme.palette.bg, rgb(0x12, 0x0f, 0x0d));
    assert_eq!(theme.preview.code.keyword, rgb(0xcf, 0x98, 0x51));
    assert_eq!(theme.preview.code.function, rgb(0x7f, 0xa7, 0xa5));

    let directory = theme.resolve(Path::new("projects"), EntryKind::Directory);
    assert_eq!(directory.class, FileClass::Directory);
    assert_eq!(directory.icon, "󰉋");
    assert_eq!(directory.color, rgb(0xcf, 0x9c, 0x67));

    let downloads = theme.resolve(Path::new("Downloads"), EntryKind::Directory);
    assert_eq!(downloads.class, FileClass::Directory);
    assert_eq!(downloads.icon, "󰉍");
    assert_eq!(downloads.color, rgb(0xd4, 0xa4, 0x66));

    let src = theme.resolve(Path::new("src"), EntryKind::Directory);
    assert_eq!(src.class, FileClass::Directory);
    assert_eq!(src.icon, "󰉋");
    assert_eq!(src.color, rgb(0xcf, 0x9c, 0x67));

    let vendor = theme.resolve(Path::new("vendor"), EntryKind::Directory);
    assert_eq!(vendor.class, FileClass::Directory);
    assert_eq!(vendor.icon, "󰉋");
    assert_eq!(vendor.color, rgb(0xcf, 0x9c, 0x67));

    let rust = theme.resolve(Path::new("main.rs"), EntryKind::File);
    assert_eq!(rust.class, FileClass::Code);
    assert_eq!(rust.icon, "");
    assert_eq!(rust.color, rgb(0xc5, 0x8a, 0x5e));
}

#[test]
fn catppuccin_mocha_example_theme_parses_as_user_theme_and_applies_palette_consistently() {
    let theme = load_alternate_example_theme("catppuccin-mocha");

    assert_eq!(theme.palette.bg, rgb(0x1e, 0x1e, 0x2e));
    assert_eq!(theme.palette.selected_bg, rgb(0x45, 0x47, 0x5a));
    assert_ne!(theme.palette.selected_bg, theme.palette.surface);
    assert_eq!(theme.preview.code.keyword, rgb(0xcb, 0xa6, 0xf7));
    assert_eq!(theme.preview.code.function, rgb(0x89, 0xb4, 0xfa));
    assert_eq!(theme.preview.code.string, rgb(0xa6, 0xe3, 0xa1));
    assert_eq!(theme.preview.code.r#type, rgb(0xf9, 0xe2, 0xaf));

    let directory = theme.resolve(Path::new("projects"), EntryKind::Directory);
    assert_eq!(directory.class, FileClass::Directory);
    assert_eq!(directory.icon, "󰉋");
    assert_eq!(directory.color, rgb(0x89, 0xb4, 0xfa));

    let downloads = theme.resolve(Path::new("Downloads"), EntryKind::Directory);
    assert_eq!(downloads.class, FileClass::Directory);
    assert_eq!(downloads.icon, "󰉍");
    assert_eq!(downloads.color, rgb(0xf9, 0xe2, 0xaf));

    let pictures = theme.resolve(Path::new("Pictures"), EntryKind::Directory);
    assert_eq!(pictures.class, FileClass::Directory);
    assert_eq!(pictures.icon, "󰉏");
    assert_eq!(pictures.color, rgb(0x94, 0xe2, 0xd5));

    let music = theme.resolve(Path::new("Music"), EntryKind::Directory);
    assert_eq!(music.class, FileClass::Directory);
    assert_eq!(music.icon, "󱍙");
    assert_eq!(music.color, rgb(0xcb, 0xa6, 0xf7));

    let src = theme.resolve(Path::new("src"), EntryKind::Directory);
    assert_eq!(src.class, FileClass::Directory);
    assert_eq!(src.icon, "󰉋");
    assert_eq!(src.color, rgb(0x89, 0xb4, 0xfa));

    let rust = theme.resolve(Path::new("main.rs"), EntryKind::File);
    assert_eq!(rust.class, FileClass::Code);
    assert_eq!(rust.icon, "");
    assert_eq!(rust.color, rgb(0xfa, 0xb3, 0x87));

    let package = theme.resolve(Path::new("package.json"), EntryKind::File);
    assert_eq!(package.class, FileClass::Config);
    assert_eq!(package.icon, "󰏗");
    assert_eq!(package.color, rgb(0x89, 0xb4, 0xfa));

    let readme = theme.resolve(Path::new("README.md"), EntryKind::File);
    assert_eq!(readme.class, FileClass::Document);
    assert_eq!(readme.icon, "");
    assert_eq!(readme.color, rgb(0xf9, 0xe2, 0xaf));
}

#[test]
fn tokyo_night_example_theme_parses_as_user_theme_and_applies_palette_consistently() {
    let theme = load_alternate_example_theme("tokyo-night");

    assert_eq!(theme.palette.bg, rgb(0x1a, 0x1b, 0x26));
    assert_eq!(theme.preview.code.keyword, rgb(0xbb, 0x9a, 0xf7));
    assert_eq!(theme.preview.code.function, rgb(0x7d, 0xcf, 0xff));
    assert_eq!(theme.preview.code.string, rgb(0x9e, 0xce, 0x6a));
    assert_eq!(theme.preview.code.r#type, rgb(0xe0, 0xaf, 0x68));

    let directory = theme.resolve(Path::new("projects"), EntryKind::Directory);
    assert_eq!(directory.class, FileClass::Directory);
    assert_eq!(directory.icon, "󰉋");
    assert_eq!(directory.color, rgb(0x7a, 0xa2, 0xf7));

    let downloads = theme.resolve(Path::new("Downloads"), EntryKind::Directory);
    assert_eq!(downloads.class, FileClass::Directory);
    assert_eq!(downloads.icon, "󰉍");
    assert_eq!(downloads.color, rgb(0xe0, 0xaf, 0x68));

    let pictures = theme.resolve(Path::new("Pictures"), EntryKind::Directory);
    assert_eq!(pictures.class, FileClass::Directory);
    assert_eq!(pictures.icon, "󰉏");
    assert_eq!(pictures.color, rgb(0x73, 0xda, 0xca));

    let music = theme.resolve(Path::new("Music"), EntryKind::Directory);
    assert_eq!(music.class, FileClass::Directory);
    assert_eq!(music.icon, "󱍙");
    assert_eq!(music.color, rgb(0xbb, 0x9a, 0xf7));

    let src = theme.resolve(Path::new("src"), EntryKind::Directory);
    assert_eq!(src.class, FileClass::Directory);
    assert_eq!(src.icon, "󰉋");
    assert_eq!(src.color, rgb(0x7a, 0xa2, 0xf7));

    let rust = theme.resolve(Path::new("main.rs"), EntryKind::File);
    assert_eq!(rust.class, FileClass::Code);
    assert_eq!(rust.icon, "");
    assert_eq!(rust.color, rgb(0xff, 0x9e, 0x64));

    let package = theme.resolve(Path::new("package.json"), EntryKind::File);
    assert_eq!(package.class, FileClass::Config);
    assert_eq!(package.icon, "󰏗");
    assert_eq!(package.color, rgb(0x7a, 0xa2, 0xf7));

    let readme = theme.resolve(Path::new("README.md"), EntryKind::File);
    assert_eq!(readme.class, FileClass::Document);
    assert_eq!(readme.icon, "");
    assert_eq!(readme.color, rgb(0xe0, 0xaf, 0x68));
}

#[test]
fn nord_example_theme_parses_as_user_theme_and_applies_palette_consistently() {
    let theme = load_alternate_example_theme("nord");

    assert_eq!(theme.palette.bg, rgb(0x2e, 0x34, 0x40));
    assert_eq!(theme.palette.chrome_alt, rgb(0x2b, 0x30, 0x3c));
    assert_eq!(theme.palette.path_bg, rgb(0x34, 0x3b, 0x49));
    assert_eq!(theme.palette.selected_bg, rgb(0x4c, 0x56, 0x6a));
    assert_eq!(theme.palette.grid_selection_band, rgb(0x87, 0x58, 0x49));
    assert_eq!(theme.palette.grid_yank_band, rgb(0x5a, 0x69, 0x4d));
    assert_eq!(theme.palette.grid_cut_band, rgb(0x99, 0x4e, 0x55));
    assert_eq!(theme.palette.accent, rgb(0x88, 0xc0, 0xd0));
    assert_eq!(theme.preview.code.line_number, rgb(0x73, 0x7f, 0x97));
    assert_eq!(theme.preview.code.keyword, rgb(0x81, 0xa1, 0xc1));
    assert_eq!(theme.preview.code.string, rgb(0xa3, 0xbe, 0x8c));

    let directory = theme.resolve(Path::new("projects"), EntryKind::Directory);
    assert_eq!(directory.class, FileClass::Directory);
    assert_eq!(directory.icon, "󰉋");
    assert_eq!(directory.color, rgb(0x88, 0xc0, 0xd0));

    let rust = theme.resolve(Path::new("main.rs"), EntryKind::File);
    assert_eq!(rust.class, FileClass::Code);
    assert_eq!(rust.icon, "");
    assert_eq!(rust.color, rgb(0xd0, 0x87, 0x70));

    let image = theme.resolve(Path::new("logo.png"), EntryKind::File);
    assert_eq!(image.class, FileClass::Image);
    assert_eq!(image.color, rgb(0xa3, 0xbe, 0x8c));
}

#[test]
fn alternate_example_themes_use_normal_folder_color_for_generic_dev_directories() {
    for label in ALTERNATE_EXAMPLE_THEME_NAMES {
        let theme = load_alternate_example_theme(label);
        assert_uses_normal_folder_color_for_generic_dev_directories(&theme, label);
    }
}
