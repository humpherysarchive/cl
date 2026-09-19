import { CATEGORY_MAP, type CategoryId } from "../lib/categories";
import { useSettings } from "../lib/settings";
import { Icon } from "./../components/Icon";
import "./CategoryView.css";

interface CategoryViewProps {
  id: CategoryId;
  onImport: () => void;
}

export function CategoryView({ id, onImport }: CategoryViewProps) {
  const cat = CATEGORY_MAP[id];
  const { settings } = useSettings();

  return (
    /* Keyed on `id` in App so React remounts and replays the enter animation. */
    <article className="view">
      <header className="view__header">
        <h1 className="view__title">{cat.label}</h1>
        <p className="view__subtitle">{cat.blurb}</p>
      </header>

      <div className="view__body">
        <div className="empty">
          <div className="empty__glyph">
            <Icon name={cat.id} size={30} />
          </div>
          <h2 className="empty__title">Nothing imported yet</h2>
          <p className="empty__text">
            Connect an iPhone or point the app at a backup folder, and your{" "}
            {cat.label.toLowerCase()} will show up here.
          </p>
          <button type="button" className="btn btn--primary" onClick={onImport}>
            <Icon name="import" size={16} />
            Import from device or backup
          </button>

          {settings.developerMode && settings.showRawPaths && (
            <p className="empty__path">
              Source domain: <code>{DEV_DOMAINS[cat.id] ?? "—"}</code>
            </p>
          )}
        </div>
      </div>
    </article>
  );
}

/* Where each category actually lives inside an iOS backup. Surfaced only in
   developer mode; the importer will use the same mapping. */
const DEV_DOMAINS: Partial<Record<CategoryId, string>> = {
  photos: "CameraRollDomain/Media/DCIM",
  videos: "CameraRollDomain/Media/DCIM",
  messages: "HomeDomain/Library/SMS/sms.db",
  contacts: "HomeDomain/Library/AddressBook/AddressBook.sqlitedb",
  calls: "HomeDomain/Library/CallHistoryDB/CallHistory.storedata",
  voicemail: "HomeDomain/Library/Voicemail/voicemail.db",
  mail: "HomeDomain/Library/Mail",
  notes: "AppDomainGroup-group.com.apple.notes/NoteStore.sqlite",
  calendar: "HomeDomain/Library/Calendar/Calendar.sqlitedb",
  reminders: "AppDomainGroup-group.com.apple.reminders",
  voicememos: "AppDomainGroup-group.com.apple.VoiceMemos",
  health: "HealthDomain/Health/healthdb_secure.sqlite",
  safari: "HomeDomain/Library/Safari/History.db",
  files: "AppDomainGroup-group.com.apple.FileProvider",
  apps: "Manifest.db (AppDomain-*)",
  music: "MediaDomain/Media/iTunes_Control",
  wallet: "HomeDomain/Library/Passes",
  locations: "RootDomain/Library/Caches/locationd",
  device: "Info.plist / Manifest.plist",
};
