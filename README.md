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

Backups — **including encrypted ones** — open, and **Photos** is browsable: a
date-grouped grid of the camera roll with a full-size viewer. The other
eighteen categories report how many files they hold and still show an empty
state; Messages is next.

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
| Layering | Light mode stacks white cards on grouped gray. Dark mode layers *upward from black*, the way iOS does, so a sheet is lighter than the window behind it. |

Fonts and icons are original or openly licensed. Nothing Apple-owned ships in
this repository.

## Encrypted backups

Encrypted backups are the ones worth having. Health data, saved passwords,
Wi-Fi settings and call history are *only* written when "Encrypt local backup"
is ticked, so for actually recovering someone's life off a dead phone the
encrypted path is the main path, not an extra.

Nothing in an encrypted backup is readable without the password — not even the
list of files. The chain the app walks:

```
backup password
  └─ PBKDF2-SHA256 (DPSL, DPIC)        iOS 10.2+ only
      └─ PBKDF2-SHA1 (SALT, ITER)      →  keybag key
          └─ AES unwrap (RFC 3394)     →  class keys, one per protection class
              ├─ unwrap ManifestKey    →  AES-256-CBC  →  Manifest.db
              └─ unwrap per-file key   →  AES-256-CBC  →  file contents
```

Details worth knowing:

- `Info.plist` stays readable even when the backup is encrypted, so the app can
  show *which phone this is* before asking for anything.
- A wrong password is reported as a wrong password. RFC 3394 carries an
  integrity check, so unwrapping fails cleanly rather than yielding plausible
  garbage.
- Keys whose `WRAP` marks them as tied to the phone's hardware key are skipped,
  not treated as failures — they are not recoverable from a backup by design.
- The decrypted `Manifest.db` is staged in a temporary file, because SQLite
  cannot read from memory, and removed as soon as its rows are read. Plaintext
  never lands in the backup folder.
- Class keys are wiped on drop, and `Backup`'s `Debug` impl is written by hand
  so a log line or panic message can't print key material.

The tests build a synthetic encrypted backup — keybag, wrapped manifest key,
encrypted `Manifest.db`, per-file keys — and read a file back through the
public API, covering the whole chain. They verify the pieces agree with each
other. **They cannot verify agreement with Apple**; that needs a real backup
from a real device, which is the next thing to test against.

## Photos

The camera roll comes from `CameraRollDomain`, `Media/DCIM/**`.
`Media/PhotoData/Photos.sqlite` supplies capture dates, pixel dimensions, and
which assets are in Recently Deleted — the file listing alone cannot tell you
that. Videos are left for the Videos category.

The schema moves between iOS releases, so nothing is assumed: the asset table
is read through `PRAGMA table_info`, it is found under either `ZASSET`
(iOS 13+) or `ZGENERICASSET` (iOS 10–12), every optional column degrades to
nothing, and an unreadable database falls back to listing `Media/DCIM` in
filename order. Values are coerced across SQLite storage classes rather than
dropped, because a declared column type is only an affinity — silently losing
`ZTRASHEDSTATE` would put deleted photos back in the grid.

**Originals only.** Edits made in the Photos app live under
`Media/PhotoData/Mutations/` and are not shown; a cropped or filtered photo
appears here as it was taken.

### HEIC

WebKitGTK cannot render HEIC, and most of a modern camera roll is HEIC, so it
is decoded in-process by [`heif-oxide`](https://crates.io/crates/heif-oxide) —
pure Rust, MIT/Apache-2.0, no C dependency. Measured against libheif on a 12 MP
10-bit file: RMSE 0.0017 (pixel-identical bar bit-depth rounding), ~400 ms to
decode, **+339 KB** of binary. A libheif binding would have cost ~1.5 MB plus a
system package on each of three platforms, a Homebrew path baked into the macOS
build, and an LGPL dependency in a repository whose license is still open.

JPEG and PNG are never transcoded — the webview renders them from the original
bytes.

### Thumbnails

Decoding is far too slow to do for a whole library up front, so thumbnails are
produced only for the cells the grid actually shows, from the cheapest source
available:

1. the on-disk cache,
2. a JPEG derivative iOS already wrote into the backup under
   `Media/PhotoData/Thumbnails/V2/`, when the backup carries one — that turns a
   HEVC decode into a file read,
3. decoding the original.

The cache lives in the OS cache directory (`~/.cache/org.humpherysarchive.cl/thumbnails/<backup>/`
on Linux), **never in the backup folder**, namespaced per backup because a file
id is only unique within one device. It is capped at 200 MB, evicted
oldest-first, and clearable from **Settings → Advanced**, which also shows the
exact location.

Concurrent decodes are capped at one fewer than the core count (max 4), so
scrolling fast through a large roll cannot leave an old machine thrashing.
Cache hits never take a slot.

### Streaming

Images reach the webview over an `archive://` URI scheme, not IPC:
`archive://localhost/thumb/<fileId>` and `/full/<fileId>` (Windows serves the
same routes over `http://archive.localhost`). A 12 MP photo is several
megabytes, and base64 over IPC would inflate it by a third and hold the whole
string in memory on both sides. Requested ids must be 40 hex characters, which
keeps path separators out of cache filenames.

The grid is windowed by hand rather than with a virtualization library: row
heights are known up front, so finding the first visible row is a binary search
and only what is on screen is ever in the DOM.

## Platforms

| Target | Bundle | Notes |
| --- | --- | --- |
| Debian / Ubuntu | `.deb`, AppImage | Built on Ubuntu 22.04 in CI, so the binary links against an older glibc and still runs on the 2014-era machines this is meant to rescue. |
| Windows | NSIS installer | |
| macOS | `.dmg` | Built twice, Apple silicon and Intel. |

`.github/workflows/build.yml` runs tests, `rustfmt`, Clippy and a typecheck,
then bundles all four. Cross-compiling macOS and Windows from Linux isn't
practical, so the matrix is how those artifacts get produced.

## Layout

```
src/
  lib/categories.ts    The 19 data classes and their sidebar grouping
  lib/settings.tsx     Settings store, theme resolution, persistence
  lib/backup.ts        Typed wrappers over the Rust commands
  lib/session.tsx      Which backup is open; remembers the path, never the password
  components/          Sidebar, Toolbar, Toggle, ImportSheet, SettingsSheet
  views/               Per-category views (PhotosView is windowed by hand)
  styles/              Tokens, shared sheet chrome, global styles
src-tauri/
  src/ios/crypto.rs    RFC 3394 key unwrap, AES-256-CBC
  src/ios/keybag.rs    Keybag parsing, key derivation, class keys
  src/ios/manifest.rs  Manifest.plist, Manifest.db, per-file key records
  src/ios/mod.rs       Opening a backup; domain -> category routing
  src/ios/photos.rs    Photos.sqlite, schema-tolerant
  src/ios/thumbs.rs    Decoding, scaling, and the thumbnail cache
  src/protocol.rs      The archive:// image scheme
  src/lib.rs           Tauri commands
```

## Development

```sh
npm test            # typecheck + Rust tests
npm run screenshot  # render the UI to PNGs without a full build
```

## Settings

- **Appearance** — Light / Dark / Auto, and a reduce-motion switch for older
  hardware.
- **Sidebar** — show or hide the sidebar, and toggle each of the 19 categories
  individually. Hiding a category only changes what's displayed; nothing is
  deleted.
- **Advanced** — the thumbnail cache's size, location and a Clear button, plus
  a single Developer mode switch. Turning that on reveals verbose logging, raw
  backup paths, temp-file retention and parser thread count. A normal user
  never sees these.

`Ctrl`/`Cmd` + `,` opens Settings.

## Roadmap

- [x] Interface shell, theming, sidebar, settings
- [x] Backup detection and `Manifest.plist` / `Info.plist` reading
- [x] Encrypted-backup keybag, key derivation and file decryption
- [x] Import flow: pick a folder, identify the phone, unlock, count what's there
- [x] Cross-platform bundles (Debian/Ubuntu, Windows, macOS) in CI
- [x] Photos: date-grouped grid, HEIC decoding, cached thumbnails, full-size viewer
- [ ] Verify against a real encrypted backup from a device
- [ ] Messages, then the remaining categories
- [ ] Export to open formats (JSON, CSV, vCard, ICS, plain image files)
- [ ] Bootable Debian live image for recovery on a machine with no working OS

## License

Not yet chosen.
