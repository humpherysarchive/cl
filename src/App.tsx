import { useEffect, useState } from "react";
import { Sidebar } from "./components/Sidebar";
import { SettingsSheet } from "./components/SettingsSheet";
import { Toolbar } from "./components/Toolbar";
import { CategoryView } from "./views/CategoryView";
import { useSettings } from "./lib/settings";
import type { CategoryId } from "./lib/categories";
import "./App.css";

export function App() {
  const { settings } = useSettings();
  const [active, setActive] = useState<CategoryId>("photos");
  const [settingsOpen, setSettingsOpen] = useState(false);

  // If the active category gets hidden from the sidebar, fall back to the
  // first one still visible rather than showing a view with no way back.
  useEffect(() => {
    if (
      settings.visibleCategories.length > 0 &&
      !settings.visibleCategories.includes(active)
    ) {
      setActive(settings.visibleCategories[0]);
    }
  }, [settings.visibleCategories, active]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === ",") {
        e.preventDefault();
        setSettingsOpen(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="app">
      <div className="app__sidebar" data-visible={settings.sidebarVisible}>
        <Sidebar
          active={active}
          onSelect={setActive}
          onOpenSettings={() => setSettingsOpen(true)}
        />
      </div>

      <main className="app__main">
        <Toolbar onImport={() => setSettingsOpen(false)} />
        <div className="app__content">
          <CategoryView key={active} id={active} onImport={() => {}} />
        </div>
      </main>

      <SettingsSheet open={settingsOpen} onClose={() => setSettingsOpen(false)} />
    </div>
  );
}
