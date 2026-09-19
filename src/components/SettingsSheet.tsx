import { Fragment, useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { cacheInfo, clearCache, type CacheInfo } from "../lib/backup";
import { CATEGORIES, GROUPS } from "../lib/categories";
import { useSettings, type ThemeChoice } from "../lib/settings";
import { Icon } from "./Icon";
import { Toggle } from "./Toggle";
import "./SettingsSheet.css";

const THEMES: { value: ThemeChoice; label: string }[] = [
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
  { value: "system", label: "Auto" },
];

interface SettingsSheetProps {
  open: boolean;
  onClose: () => void;
}

export function SettingsSheet({ open, onClose }: SettingsSheetProps) {
  const { settings, set, toggleCategory, setAllCategories, reset } = useSettings();
  const sheetRef = useRef<HTMLDivElement>(null);
  const [cache, setCache] = useState<CacheInfo | null>(null);
  const [clearing, setClearing] = useState(false);

  const refreshCache = useCallback(() => {
    cacheInfo()
      .then(setCache)
      .catch(() => setCache(null));
  }, []);

  // Read the size when the sheet opens rather than on every render, so the
  // directory is not walked while the user is scrolling.
  useEffect(() => {
    if (open) refreshCache();
  }, [open, refreshCache]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    sheetRef.current?.focus();
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  const allOn = settings.visibleCategories.length === CATEGORIES.length;

  return (
    <div className="sheet-layer" data-open={open} aria-hidden={!open}>
      <button
        type="button"
        className="sheet-scrim"
        aria-label="Close settings"
        tabIndex={open ? 0 : -1}
        onClick={onClose}
      />

      <div
        className="sheet"
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        tabIndex={-1}
        ref={sheetRef}
      >
        <header className="sheet__bar">
          <span className="sheet__grabber" />
          <h2 className="sheet__title">Settings</h2>
          <button
            type="button"
            className="sheet__close"
            onClick={onClose}
            aria-label="Close settings"
            tabIndex={open ? 0 : -1}
          >
            <Icon name="close" size={16} />
          </button>
        </header>

        <div className="sheet__body scroll">
          <Group title="Appearance">
            <Row label="Theme" detail="Match the system, or pick one and stay there.">
              <div
                className="segmented"
                style={{
                  ["--seg-index" as string]: THEMES.findIndex(
                    (t) => t.value === settings.theme,
                  ),
                }}
              >
                <span className="segmented__pill" />
                {THEMES.map((t) => (
                  <button
                    key={t.value}
                    type="button"
                    className="segmented__option"
                    aria-pressed={settings.theme === t.value}
                    tabIndex={open ? 0 : -1}
                    onClick={() => set("theme", t.value)}
                  >
                    {t.label}
                  </button>
                ))}
              </div>
            </Row>

            <Row
              label="Reduce motion"
              detail="Cross-fade instead of sliding. Easier on older machines."
            >
              <Toggle
                label="Reduce motion"
                checked={settings.reduceMotion}
                onChange={(v) => set("reduceMotion", v)}
              />
            </Row>
          </Group>

          <Group
            title="Sidebar"
            footer="Hidden categories stay on disk — this only changes what you see."
            action={
              <button
                type="button"
                className="group__action"
                tabIndex={open ? 0 : -1}
                onClick={() => setAllCategories(!allOn)}
              >
                {allOn ? "Hide all" : "Show all"}
              </button>
            }
          >
            <Row label="Show sidebar" detail="Hide it for a full-width view.">
              <Toggle
                label="Show sidebar"
                checked={settings.sidebarVisible}
                onChange={(v) => set("sidebarVisible", v)}
              />
            </Row>

            {GROUPS.map((group) => (
              <Fragment key={group.id}>
                <div className="group__subhead">{group.label}</div>
                {CATEGORIES.filter((c) => c.group === group.id).map((cat) => (
                  <Row key={cat.id} label={cat.label} icon={cat.id}>
                    <Toggle
                      label={`Show ${cat.label}`}
                      checked={settings.visibleCategories.includes(cat.id)}
                      onChange={() => toggleCategory(cat.id)}
                    />
                  </Row>
                ))}
              </Fragment>
            ))}
          </Group>

          <Group
            title="Advanced"
            footer={
              cache
                ? `Thumbnails are kept in ${cache.location}, never in the backup folder. They rebuild automatically.`
                : "Only turn on Developer mode if someone walking you through a problem asks you to."
            }
          >
            <Row
              label="Thumbnail cache"
              icon="photos"
              detail={
                cache
                  ? `${formatBytes(cache.bytes)} of ${formatBytes(cache.limit)} used`
                  : "Not available until a backup is open."
              }
            >
              <button
                type="button"
                className="group__action"
                tabIndex={open ? 0 : -1}
                disabled={!cache || cache.bytes === 0 || clearing}
                onClick={async () => {
                  setClearing(true);
                  try {
                    await clearCache();
                  } finally {
                    setClearing(false);
                    refreshCache();
                  }
                }}
              >
                {clearing ? "Clearing…" : "Clear"}
              </button>
            </Row>

            <Row label="Developer mode" icon="wrench">
              <Toggle
                label="Developer mode"
                checked={settings.developerMode}
                onChange={(v) => set("developerMode", v)}
              />
            </Row>
          </Group>

          <div className="reveal" data-open={settings.developerMode}>
            <div className="reveal__inner">
              <Group title="Developer" footer="These settings are not supported and can slow imports down.">
                <Row label="Verbose logging" detail="Write every parser step to the log file.">
                  <Toggle
                    label="Verbose logging"
                    checked={settings.verboseLogging}
                    onChange={(v) => set("verboseLogging", v)}
                  />
                </Row>
                <Row label="Show raw backup paths" detail="Display the on-disk domain and hash for each item.">
                  <Toggle
                    label="Show raw backup paths"
                    checked={settings.showRawPaths}
                    onChange={(v) => set("showRawPaths", v)}
                  />
                </Row>
                <Row label="Keep temporary files" detail="Leave extracted databases behind after an import.">
                  <Toggle
                    label="Keep temporary files"
                    checked={settings.keepTempFiles}
                    onChange={(v) => set("keepTempFiles", v)}
                  />
                </Row>
                <Row label="Parser threads" detail="Higher is faster, up to what the machine can take.">
                  <Stepper
                    value={settings.parserConcurrency}
                    min={1}
                    max={16}
                    onChange={(v) => set("parserConcurrency", v)}
                  />
                </Row>
              </Group>
            </div>
          </div>

          <Group>
            <button
              type="button"
              className="row row--button row--danger"
              tabIndex={open ? 0 : -1}
              onClick={reset}
            >
              Reset all settings
            </button>
          </Group>

          <p className="sheet__version">Archive 0.1.0</p>
        </div>
      </div>
    </div>
  );
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

function Group({
  title,
  footer,
  action,
  children,
}: {
  title?: string;
  footer?: ReactNode;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="group">
      {(title || action) && (
        <div className="group__head">
          {title && <h3 className="group__title">{title}</h3>}
          {action}
        </div>
      )}
      <div className="group__card">{children}</div>
      {footer && <p className="group__footer">{footer}</p>}
    </section>
  );
}

function Row({
  label,
  detail,
  icon,
  children,
}: {
  label: string;
  detail?: string;
  icon?: Parameters<typeof Icon>[0]["name"];
  children: ReactNode;
}) {
  return (
    <div className="row">
      {icon && <Icon name={icon} size={17} className="row__icon" />}
      <div className="row__text">
        <span className="row__label">{label}</span>
        {detail && <span className="row__detail">{detail}</span>}
      </div>
      <div className="row__control">{children}</div>
    </div>
  );
}

function Stepper({
  value,
  min,
  max,
  onChange,
}: {
  value: number;
  min: number;
  max: number;
  onChange: (n: number) => void;
}) {
  return (
    <div className="stepper">
      <button
        type="button"
        onClick={() => onChange(Math.max(min, value - 1))}
        disabled={value <= min}
        aria-label="Decrease"
      >
        −
      </button>
      <span className="stepper__value">{value}</span>
      <button
        type="button"
        onClick={() => onChange(Math.min(max, value + 1))}
        disabled={value >= max}
        aria-label="Increase"
      >
        +
      </button>
    </div>
  );
}
