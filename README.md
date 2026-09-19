# Archive

A desktop app for getting an iPhone's contents off the phone and onto a
computer you control — photos, messages, contacts, notes, calendars, health
data and the rest — and then browsing them like an ordinary app rather than a
forensics tool.

The interface is deliberately plain: a category list down the left side, a
large title, and one obvious button. Anything that needs explaining lives in
Settings, and anything that needs explaining *twice* lives behind Developer
mode, off by default.

## Status

Interface shell is built and runnable. The importer is not written yet — every
category currently shows an empty state. See [Roadmap](#roadmap).

## Running it

Requires Node 20+ and Rust 1.77+.

```sh
npm install
npm run tauri dev
```

On Debian/Ubuntu the Tauri build also needs the system webview headers:

```sh
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libxdo-dev \
                 libssl-dev librsvg2-dev pkg-config
```

To produce installers:

```sh
npm run tauri build
```

## Why Tauri

The app has to run on machines from roughly 2014 onward and ship as part of a
bootable USB image, so size and idle memory are features, not details. Tauri
renders through the OS's existing webview instead of bundling a browser: the
binary is a few megabytes rather than ~180 MB, and idle memory is a fraction of
the Electron equivalent. The UI is still plain HTML and CSS, which is what makes
the iOS-style motion straightforward.

## Design

| Concern | Decision |
| --- | --- |
| Palette | iOS system colors, defined once in `src/styles/tokens.css`. No component hardcodes a color. |
| Accent | System blue (`#007AFF` light, `#0A84FF` dark), used on view titles and active sidebar rows. |
| Type | [Inter](https://rsms.me/inter/), SIL Open Font License. Metrically close to SF Pro, with `cv05`/`cv11`/`ss03` enabled for the single-storey shapes that read as a system font. Bundled locally, so it works with no network. |
| Icons | Hand-drawn on a 24px grid at 1.7px stroke (`src/components/Icon.tsx`). Original geometry — SF Symbols are Apple-licensed and are not used. |
| Motion | `--ease-spring` is the iOS sheet curve: quick start, long settle, no overshoot. Toggles use `--ease-bounce`. All durations collapse to 1ms under `prefers-reduced-motion` or the in-app **Reduce motion** setting. |

Fonts and icons are original or openly licensed. Nothing Apple-owned ships in
this repository.

## Layout

```
src/
  lib/categories.ts    The 19 data classes and their sidebar grouping
  lib/settings.tsx     Settings store, theme resolution, persistence
  components/          Sidebar, Toolbar, Toggle, SettingsSheet, Icon
  views/               Per-category views
  styles/              Tokens and global styles
src-tauri/
  src/ios.rs           iOS backup detection (parsers land here)
  src/lib.rs           Tauri commands
```

## Settings

- **Appearance** — Light / Dark / Auto, and a reduce-motion switch for older
  hardware.
- **Sidebar** — show or hide the sidebar, and toggle each of the 19 categories
  individually. Hiding a category only changes what's displayed; nothing is
  deleted.
- **Advanced** — a single Developer mode switch. Turning it on reveals verbose
  logging, raw backup paths, temp-file retention and parser thread count. A
  normal user never sees these.

`Ctrl`/`Cmd` + `,` opens Settings.

## Roadmap

- [x] Interface shell, theming, sidebar, settings
- [ ] Backup detection and `Manifest.plist` / `Info.plist` reading
- [ ] Encrypted-backup keybag and file decryption
- [ ] Per-category parsers (Photos, Messages, Contacts, Notes, Calendar, Health, …)
- [ ] Export to open formats (JSON, CSV, vCard, ICS, plain image files)
- [ ] Bootable Debian live image for recovery on a machine with no working OS

## License

Not yet chosen.
