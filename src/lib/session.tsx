import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import {
  closeBackup,
  type BackupSummary,
  type OpenedBackup,
} from "./backup";

/**
 * Which backup is currently open.
 *
 * The folder path is remembered between launches so the app can offer to
 * reopen it. The backup password is never stored — an encrypted backup is
 * re-prompted for on every launch, which is the whole point of it being
 * encrypted.
 */
export interface Session {
  device: BackupSummary | null;
  opened: OpenedBackup | null;
}

const LAST_PATH_KEY = "cl.lastBackup.v1";

interface Remembered {
  path: string;
  deviceName: string | null;
}

function loadRemembered(): Remembered | null {
  try {
    const raw = localStorage.getItem(LAST_PATH_KEY);
    return raw ? (JSON.parse(raw) as Remembered) : null;
  } catch {
    return null;
  }
}

interface SessionContextValue extends Session {
  /** The last backup folder used, for the reopen shortcut. */
  remembered: Remembered | null;
  setOpened: (device: BackupSummary, opened: OpenedBackup) => void;
  close: () => Promise<void>;
  forget: () => void;
}

const SessionContext = createContext<SessionContextValue | null>(null);

export function SessionProvider({ children }: { children: ReactNode }) {
  const [device, setDevice] = useState<BackupSummary | null>(null);
  const [opened, setOpenedState] = useState<OpenedBackup | null>(null);
  const [remembered, setRemembered] = useState<Remembered | null>(loadRemembered);

  const setOpened = useCallback(
    (nextDevice: BackupSummary, nextOpened: OpenedBackup) => {
      setDevice(nextDevice);
      setOpenedState(nextOpened);

      const entry: Remembered = {
        path: nextDevice.path,
        deviceName: nextDevice.deviceName,
      };
      setRemembered(entry);
      try {
        localStorage.setItem(LAST_PATH_KEY, JSON.stringify(entry));
      } catch {
        /* Private mode: the app still works, it just forgets the path. */
      }
    },
    [],
  );

  const close = useCallback(async () => {
    setDevice(null);
    setOpenedState(null);
    try {
      await closeBackup();
    } catch {
      /* Already closed, or the backend is gone; nothing to recover. */
    }
  }, []);

  const forget = useCallback(() => {
    setRemembered(null);
    try {
      localStorage.removeItem(LAST_PATH_KEY);
    } catch {
      /* Nothing to do. */
    }
  }, []);

  // Drop the decrypted class keys when the window goes away rather than
  // leaving them in memory for as long as the process lingers.
  useEffect(() => {
    const onUnload = () => void closeBackup().catch(() => {});
    window.addEventListener("beforeunload", onUnload);
    return () => window.removeEventListener("beforeunload", onUnload);
  }, []);

  const value = useMemo(
    () => ({ device, opened, remembered, setOpened, close, forget }),
    [device, opened, remembered, setOpened, close, forget],
  );

  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}

export function useSession() {
  const ctx = useContext(SessionContext);
  if (!ctx) throw new Error("useSession must be used inside <SessionProvider>");
  return ctx;
}
