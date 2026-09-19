import { useSession } from "../lib/session";
import { useSettings } from "../lib/settings";
import { Icon } from "./Icon";
import "./Toolbar.css";

interface ToolbarProps {
  onImport: () => void;
}

export function Toolbar({ onImport }: ToolbarProps) {
  const { settings, set, resolvedTheme } = useSettings();
  const { device } = useSession();

  return (
    /* data-tauri-drag-region makes the empty space behave like a title bar. */
    <div className="toolbar" data-tauri-drag-region>
      <button
        type="button"
        className="toolbar__icon-btn"
        aria-label={settings.sidebarVisible ? "Hide sidebar" : "Show sidebar"}
        aria-pressed={settings.sidebarVisible}
        onClick={() => set("sidebarVisible", !settings.sidebarVisible)}
      >
        <Icon name="sidebar" size={17} />
      </button>

      <div className="toolbar__search">
        <Icon name="search" size={14} className="toolbar__search-icon" />
        <input type="search" placeholder="Search" aria-label="Search" />
      </div>

      <div className="toolbar__spacer" data-tauri-drag-region />

      <button
        type="button"
        className="toolbar__icon-btn"
        aria-label={`Switch to ${resolvedTheme === "dark" ? "light" : "dark"} mode`}
        onClick={() => set("theme", resolvedTheme === "dark" ? "light" : "dark")}
      >
        {/* Both glyphs are mounted so the swap can cross-rotate. */}
        <span className="toolbar__theme" data-dark={resolvedTheme === "dark"}>
          <Icon name="sun" size={17} className="toolbar__theme-sun" />
          <Icon name="moon" size={17} className="toolbar__theme-moon" />
        </span>
      </button>

      {/* Once a backup is open the button says which phone it is, and acts as
          the way to switch to another one. */}
      {device ? (
        <button
          type="button"
          className="toolbar__device"
          onClick={onImport}
          title={device.path}
        >
          <Icon name="device" size={15} />
          <span>{device.deviceName ?? "iPhone"}</span>
          <Icon name="chevron" size={12} className="toolbar__device-chevron" />
        </button>
      ) : (
        <button type="button" className="toolbar__import" onClick={onImport}>
          <Icon name="import" size={15} />
          Import
        </button>
      )}
    </div>
  );
}
