Desktop app for getting an iPhone's data onto a computer you control —
including **encrypted** backups.

### What works

- **Photos** — the camera roll as a date-grouped grid, with a full-size
  viewer. HEIC is decoded in-process, so it works on Linux where the system
  webview cannot render it. Thumbnails are cached; Recently Deleted is
  excluded; videos are left for their own category. Originals only: edits
  made in the Photos app are not applied.
- **Encrypted backups** — keybag parsing, key derivation, RFC 3394 key
  unwrapping and AES-256-CBC decryption of the file index and file contents.
  The backup folder is remembered between launches; the password never is.
- **Every category** reports how many files it holds.

### What doesn't yet

Messages and the remaining sixteen categories still show an empty state. They
are counted, not rendered.

### Install

**Debian / Ubuntu**

```sh
sudo apt install ./Archive_*_amd64.deb
```

Or take the `.AppImage`, `chmod +x` it, and run it in place. The AppImage is
much larger because it carries its own copy of the system webview.

**macOS** — one universal build for Intel and Apple silicon. It is not
code-signed or notarised, so Gatekeeper will refuse it until you clear the
quarantine flag:

```sh
xattr -dr com.apple.quarantine /Applications/Archive.app
```

**Windows** — SmartScreen will warn for the same reason. Choose *More info* →
*Run anyway*.

Nothing leaves your machine: the app has no network access, reads only the
backup folder you point it at, and writes only to its own cache directory.
