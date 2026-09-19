import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { CATEGORIES, DEFAULT_VISIBLE, type CategoryId } from "./categories";

export type ThemeChoice = "light" | "dark" | "system";

export interface Settings {
  theme: ThemeChoice;
  /** Categories shown in the sidebar. Order always follows CATEGORIES. */
  visibleCategories: CategoryId[];
  sidebarVisible: boolean;
  reduceMotion: boolean;

  /* Everything below lives behind the Developer section and is off by
     default. A normal user never has to know these exist. */
  developerMode: boolean;
  verboseLogging: boolean;
  showRawPaths: boolean;
  keepTempFiles: boolean;
  parserConcurrency: number;
}

const DEFAULTS: Settings = {
  theme: "system",
  visibleCategories: DEFAULT_VISIBLE,
  sidebarVisible: true,
  reduceMotion: false,
  developerMode: false,
  verboseLogging: false,
  showRawPaths: false,
  keepTempFiles: false,
  parserConcurrency: 4,
};

const STORAGE_KEY = "cl.settings.v1";

function load(): Settings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULTS;
    const parsed = JSON.parse(raw) as Partial<Settings>;
    // Drop category ids from older builds so a rename can't hide the sidebar.
    const known = new Set(CATEGORIES.map((c) => c.id));
    const visible = (parsed.visibleCategories ?? DEFAULTS.visibleCategories)
      .filter((id): id is CategoryId => known.has(id as CategoryId));
    return { ...DEFAULTS, ...parsed, visibleCategories: visible };
  } catch {
    return DEFAULTS;
  }
}

interface SettingsContextValue {
  settings: Settings;
  set: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
  toggleCategory: (id: CategoryId) => void;
  setAllCategories: (visible: boolean) => void;
  reset: () => void;
  /** Resolved light/dark after applying the "system" choice. */
  resolvedTheme: "light" | "dark";
}

const SettingsContext = createContext<SettingsContextValue | null>(null);

export function SettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<Settings>(load);
  const [systemTheme, setSystemTheme] = useState<"light" | "dark">(() =>
    window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light",
  );

  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = (e: MediaQueryListEvent) =>
      setSystemTheme(e.matches ? "dark" : "light");
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
    } catch {
      /* Private mode or a locked profile — the app still works, it just forgets. */
    }
  }, [settings]);

  const resolvedTheme =
    settings.theme === "system" ? systemTheme : settings.theme;

  useEffect(() => {
    document.documentElement.dataset.theme = resolvedTheme;
    // Defer the transition class by one frame so the initial paint doesn't
    // animate from the default palette into the stored one.
    const id = requestAnimationFrame(() => {
      document.body.dataset.themeReady = "true";
    });
    return () => cancelAnimationFrame(id);
  }, [resolvedTheme]);

  useEffect(() => {
    document.documentElement.dataset.reduceMotion = String(settings.reduceMotion);
  }, [settings.reduceMotion]);

  const set = useCallback(
    <K extends keyof Settings>(key: K, value: Settings[K]) =>
      setSettings((s) => ({ ...s, [key]: value })),
    [],
  );

  const toggleCategory = useCallback((id: CategoryId) => {
    setSettings((s) => {
      const has = s.visibleCategories.includes(id);
      const next = has
        ? s.visibleCategories.filter((c) => c !== id)
        : [...s.visibleCategories, id];
      // Keep registry order rather than click order.
      const ordered = CATEGORIES.map((c) => c.id).filter((c) => next.includes(c));
      return { ...s, visibleCategories: ordered };
    });
  }, []);

  const setAllCategories = useCallback((visible: boolean) => {
    setSettings((s) => ({
      ...s,
      visibleCategories: visible ? CATEGORIES.map((c) => c.id) : [],
    }));
  }, []);

  const reset = useCallback(() => setSettings(DEFAULTS), []);

  const value = useMemo(
    () => ({ settings, set, toggleCategory, setAllCategories, reset, resolvedTheme }),
    [settings, set, toggleCategory, setAllCategories, reset, resolvedTheme],
  );

  return (
    <SettingsContext.Provider value={value}>{children}</SettingsContext.Provider>
  );
}

export function useSettings() {
  const ctx = useContext(SettingsContext);
  if (!ctx) throw new Error("useSettings must be used inside <SettingsProvider>");
  return ctx;
}
