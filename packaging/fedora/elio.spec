%bcond_with check
%global fallback_version 1.12.0
%global fallback_release 1

Name:           elio
Version:        %{?elio_version}%{!?elio_version:%{fallback_version}}
Release:        %{?elio_release}%{!?elio_release:%{fallback_release}}%{?dist}
Summary:        Terminal-native file manager with rich previews and inline images

License:        MIT
URL:            https://github.com/elio-fm/elio
Source0:        %{name}-%{version}.tar.gz
Source1:        vendor-%{version}.tar.zst

BuildRequires:  cargo-rpm-macros
BuildRequires:  cargo >= 1.95
BuildRequires:  rust >= 1.95
BuildRequires:  gcc
BuildRequires:  pkgconf-pkg-config
BuildRequires:  zstd
BuildRequires:  desktop-file-utils
Requires:       hicolor-icon-theme

%description
elio is a snappy, batteries-included terminal file manager with rich previews,
inline images, bulk actions, and trash support.

%prep
%autosetup -a 1
%cargo_prep -v vendor

%build
%cargo_build

%install
install -Dpm0755 target/rpm/%{name} %{buildroot}%{_bindir}/%{name}
install -Dpm0644 packaging/linux/%{name}.desktop %{buildroot}%{_datadir}/applications/%{name}.desktop
for size in 48 128 256 512; do
    install -Dpm0644 packaging/linux/icons/hicolor/${size}x${size}/apps/%{name}.png %{buildroot}%{_datadir}/icons/hicolor/${size}x${size}/apps/%{name}.png
done

%check
desktop-file-validate packaging/linux/%{name}.desktop
%if %{with check}
%cargo_test
%endif

%files
%license LICENSE-MIT
%doc README.md CHANGELOG.md
%{_bindir}/elio
%{_datadir}/applications/%{name}.desktop
%{_datadir}/icons/hicolor/*/apps/%{name}.png

%changelog
* Mon Aug 24 2026 Miguel Regueiro <miguelpr4242@gmail.com> - 1.12.0-1
- Add Duplicate Finder, bulk archive extraction, and preview pane controls
- Add invoking-user-aware sudo/doas sessions and config/theme path overrides
- Add a Nord example theme and improve chooser and image preview behavior
- Fix pasted input, SQLite sidecar creation, and macOS Trash restore collisions

* Wed Jul 22 2026 Miguel Regueiro <miguelpr4242@gmail.com> - 1.11.2-1
- Fix 32-bit builds by avoiding overflow in compact directory count formatting

* Sat Jul 18 2026 Miguel Regueiro <miguelpr4242@gmail.com> - 1.11.1-1
- Switch syntax highlighting to the pure-Rust regex backend and remove the native Oniguruma dependency

* Sat Jul 18 2026 Miguel Regueiro <miguelpr4242@gmail.com> - 1.11.0-1
- Add archive creation for zip, 7z, and tar variants with password prompts
- Add Kitty drag-and-drop, configurable open rules, and editor-based rename
- Add Markdown frontmatter, Quarto, Astro, Jupyter notebook, subtitle, and Coccinelle previews
- Improve themes and overlays, and fix tmux clipboard copy and file type icons

* Tue Jun 30 2026 Miguel Regueiro <miguelpr4242@gmail.com> - 1.10.0-1
- Add native archive extraction for zip, 7z, tar variants, and rar archives
- Add configurable Go To entries and a current-directory filter
- Improve Fuzzy Find, Help, Places, Open With, archive previews, and video redraws
- Fix chooser stdout output, startup video thumbnails, and long-name text editing

* Wed Jun 17 2026 Miguel Regueiro <miguelpr4242@gmail.com> - 1.9.0-1
- Add chooser file mode, file-path startup focusing, and persistent multi-directory selection
- Add git branch footer state, directory Open With, symlink actions, and default delete shortcuts
- Add LaTeX, TeX, and BibTeX previews and improve Open With navigation and fallbacks
- Fix UI freezes, narrow-layout/status behavior, Caps Lock input, and selection/paste edge cases

* Sat Jun 06 2026 Miguel Regueiro <miguelpr4242@gmail.com> - 1.8.0-1
- Add configurable multi-binding key actions, unbinding, modifiers, and named keys
- Add configurable navigation, browser control, search, history, restore, open-or-enter, and quit-without-cd bindings
- Warn for unknown key action names while preserving valid bindings
- Update help overlay labels for configurable bindings

* Sat May 30 2026 Miguel Regueiro <miguelpr4242@gmail.com> - 1.7.0-1
- Add installable bash, zsh, fish, and Nushell cd-on-exit integration
- Add terminal ANSI theme support and update example themes
- Simplify directory preview headers and improve popup/image layering
- Fix Linux/BSD terminal app opening and selected-item double-click behavior

* Fri May 22 2026 Miguel Regueiro <miguelpr4242@gmail.com> - 1.6.0-1
- Add shell-in-current-folder action and bulk opening for selected items
- Add symlink-aware browser, preview, Places, and theme rendering
- Improve fuzzy search streaming, scan limits, and symlink coverage
- Fix quit stalls from special files and large archive previews

* Fri May 15 2026 Miguel Regueiro <miguelpr4242@gmail.com> - 1.5.1-1
- Add Linux desktop entry metadata and hicolor application icons
- Add amd64 Debian package assets and official apt repository publishing

* Thu May 14 2026 Miguel Regueiro <miguelpr4242@gmail.com> - 1.5.0-1
- Add zoxide directory jumps
- Add transparent theme color values and chip text palette control
- Improve image previews inside tmux with automatic passthrough setup and expanded Sixel, iTerm inline, and Kitty direct-placement handling
- Fix Windows Terminal Sixel preview sizing on WSL
- Improve bundled light-theme chip contrast

* Sun May 03 2026 Miguel Regueiro <miguelpr4242@gmail.com> - 1.4.0-1
- Add startup directory argument
- Add keyboard preview scrolling and updated preview controls
- Fix Linux/BSD default app dispatch, Warp previews, and tmux Kitty passthrough

* Tue Apr 28 2026 Miguel Regueiro <miguelpr4242@gmail.com> - 1.3.0-1
- Add Konsole inline image preview support
- Fix crates.io installs by upgrading lofty to 0.24

* Sat Apr 25 2026 Miguel Regueiro <miguelpr4242@gmail.com> - 1.2.0-1
- Add QML preview support and icon coverage
- Add font metadata previews
- Improve large-directory navigation responsiveness
- Fix browser license icons and truncated directory preview totals

* Sat Apr 18 2026 Miguel Regueiro <miguelpr4242@gmail.com> - 1.1.0-1
- Add RAR, comic archive, and Kindle ebook previews
- Improve fuzzy search refresh and large-tree responsiveness
- Clarify metadata preview sections
- Improve trash handling and documentation

* Sun Apr 12 2026 Miguel Regueiro <miguelpr4242@gmail.com> - 1.0.1-1
- Add CLI help and version flags
- Add release packaging automation

* Sat Apr 11 2026 Miguel Regueiro <miguelpr4242@gmail.com> - 1.0.0-1
- Initial COPR package
