import { useEffect, useRef, useState } from "react";
import { CATEGORY_MAP, type CategoryId } from "../lib/categories";
import {
  chooseFolder,
  inTauri,
  inspectBackup,
  openBackup,
  type BackupSummary,
  type OpenedBackup,
} from "../lib/backup";
import { useSession } from "../lib/session";
import { Icon } from "./Icon";
import "./ImportSheet.css";

type Stage =
  | { name: "choose" }
  | { name: "found"; summary: BackupSummary }
  | { name: "opening" }
  | { name: "done"; summary: BackupSummary; result: OpenedBackup };

interface ImportSheetProps {
  open: boolean;
  onClose: () => void;
}

export function ImportSheet({ open, onClose }: ImportSheetProps) {
  const { remembered, setOpened, close, forget } = useSession();
  const [stage, setStage] = useState<Stage>({ name: "choose" });
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const passwordRef = useRef<HTMLInputElement>(null);

  // Reset between openings so a previous password never lingers in the field.
  useEffect(() => {
    if (!open) return;
    setStage({ name: "choose" });
    setPassword("");
    setError(null);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  useEffect(() => {
    if (stage.name === "found" && stage.summary.encrypted) passwordRef.current?.focus();
  }, [stage]);

  async function pick() {
    setError(null);
    try {
      const path = await chooseFolder();
      if (!path) return;
      await load(path);
    } catch (e) {
      setError(String(e));
    }
  }

  /** Inspect a folder and move to the confirmation step. */
  async function load(path: string) {
    const summary = await inspectBackup(path);
    if (!summary) {
      setError("That folder isn't an iPhone backup. Look for one containing Manifest.plist.");
      return;
    }
    // Opening a different backup replaces the current one.
    await close();
    setStage({ name: "found", summary });
  }

  async function reopen(path: string) {
    setError(null);
    try {
      await load(path);
    } catch (e) {
      setError(String(e));
      forget();
    }
  }

  async function unlock(summary: BackupSummary) {
    setError(null);
    setStage({ name: "opening" });
    try {
      const result = await openBackup(summary.path, password);
      setPassword(""); // don't keep it in memory once it has been used
      setOpened(summary, result);
      setStage({ name: "done", summary, result });
    } catch (e) {
      setError(String(e));
      setStage({ name: "found", summary });
    }
  }

  return (
    <div className="sheet-layer" data-open={open} aria-hidden={!open}>
      <button
        type="button"
        className="sheet-scrim"
        aria-label="Cancel import"
        tabIndex={open ? 0 : -1}
        onClick={onClose}
      />

      <div className="sheet sheet--short" role="dialog" aria-modal="true" aria-label="Import">
        <header className="sheet__bar">
          <span className="sheet__grabber" />
          <h2 className="sheet__title">Import</h2>
          <button
            type="button"
            className="sheet__close"
            onClick={onClose}
            aria-label="Cancel import"
            tabIndex={open ? 0 : -1}
          >
            <Icon name="close" size={16} />
          </button>
        </header>

        <div className="sheet__body scroll">
          {stage.name === "choose" && (
            <div className="import__step">
              <div className="import__glyph">
                <Icon name="import" size={28} />
              </div>
              <h3 className="import__headline">Choose a backup</h3>
              <p className="import__text">
                Pick the folder your iPhone backup is in. It's usually here:
              </p>
              <dl className="import__paths">
                <dt>macOS</dt>
                <dd>~/Library/Application Support/MobileSync/Backup</dd>
                <dt>Windows</dt>
                <dd>%APPDATA%\Apple\MobileSync\Backup</dd>
              </dl>
              {remembered && inTauri && (
                /* The folder is remembered between launches; the backup
                   password never is, so an encrypted backup asks again. */
                <button
                  type="button"
                  className="import__recent"
                  tabIndex={open ? 0 : -1}
                  onClick={() => reopen(remembered.path)}
                >
                  <Icon name="device" size={17} />
                  <span>
                    <b>{remembered.deviceName ?? "Last backup"}</b>
                    <small>{remembered.path}</small>
                  </span>
                  <Icon name="chevron" size={13} />
                </button>
              )}

              <button
                type="button"
                className={remembered && inTauri ? "btn btn--quiet" : "btn btn--primary"}
                tabIndex={open ? 0 : -1}
                disabled={!inTauri}
                onClick={pick}
              >
                Choose {remembered && inTauri ? "another" : ""} folder
              </button>
              {!inTauri && (
                /* `vite dev` in a browser tab has no file picker and no
                   backend; say so rather than throwing on click. */
                <p className="import__hint import__hint--center">
                  Importing needs the desktop app — run <code>npm run tauri dev</code>.
                </p>
              )}
            </div>
          )}

          {stage.name === "found" && (
            <div className="import__step">
              <div className="import__device">
                <Icon name="device" size={26} />
                <div>
                  <span className="import__device-name">
                    {stage.summary.deviceName ?? "iPhone"}
                  </span>
                  <span className="import__device-meta">
                    {[stage.summary.productType, stage.summary.iosVersion &&
                      `iOS ${stage.summary.iosVersion}`]
                      .filter(Boolean)
                      .join(" · ") || stage.summary.path}
                  </span>
                </div>
                {stage.summary.encrypted && <span className="badge">Encrypted</span>}
              </div>

              {stage.summary.encrypted ? (
                <form
                  className="import__form"
                  onSubmit={(e) => {
                    e.preventDefault();
                    unlock(stage.summary);
                  }}
                >
                  <label className="import__label" htmlFor="backup-password">
                    Backup password
                  </label>
                  <input
                    id="backup-password"
                    ref={passwordRef}
                    type="password"
                    className="import__input"
                    value={password}
                    autoComplete="off"
                    tabIndex={open ? 0 : -1}
                    onChange={(e) => setPassword(e.target.value)}
                  />
                  <p className="import__hint">
                    This is the password set when the backup was encrypted — not
                    the phone's passcode, and not an Apple account password.
                  </p>
                  <button
                    type="submit"
                    className="btn btn--primary"
                    disabled={password.length === 0}
                    tabIndex={open ? 0 : -1}
                  >
                    Unlock and import
                  </button>
                </form>
              ) : (
                <button
                  type="button"
                  className="btn btn--primary"
                  tabIndex={open ? 0 : -1}
                  onClick={() => unlock(stage.summary)}
                >
                  Import
                </button>
              )}
            </div>
          )}

          {stage.name === "opening" && (
            <div className="import__step">
              <div className="spinner" aria-hidden="true" />
              <h3 className="import__headline">Unlocking</h3>
              <p className="import__text">
                Working out the backup's keys. This takes a moment — the
                encryption is deliberately slow to attack.
              </p>
            </div>
          )}

          {stage.name === "done" && (
            <div className="import__step import__step--wide">
              <h3 className="import__headline">
                Found {stage.result.fileCount.toLocaleString()} files
              </h3>
              <ul className="import__counts">
                {stage.result.categories.map((c) => (
                  <li key={c.category}>
                    <Icon name={c.category as CategoryId} size={17} />
                    <span>{CATEGORY_MAP[c.category as CategoryId]?.label ?? c.category}</span>
                    <b>{c.files.toLocaleString()}</b>
                  </li>
                ))}
              </ul>
              <button
                type="button"
                className="btn btn--primary"
                tabIndex={open ? 0 : -1}
                onClick={onClose}
              >
                Done
              </button>
            </div>
          )}

          {error && (
            <p className="import__error" role="alert">
              {error}
            </p>
          )}
        </div>
      </div>
    </div>
  );
}
