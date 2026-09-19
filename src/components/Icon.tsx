/* Hand-drawn icon set. Deliberately not SF Symbols — same 24px optical grid
   and stroke weight, original geometry, so nothing Apple-licensed ships. */

import type { JSX } from "react";

export type IconName =
  | "photos" | "videos" | "messages" | "contacts" | "calls" | "voicemail"
  | "mail" | "notes" | "calendar" | "reminders" | "voicememos" | "health"
  | "safari" | "files" | "apps" | "music" | "wallet" | "locations" | "device"
  | "settings" | "sun" | "moon" | "chevron" | "search" | "import" | "close"
  | "sidebar" | "wrench";

const P: Record<IconName, JSX.Element> = {
  photos: <><rect x="3" y="5" width="18" height="14" rx="3.2" /><circle cx="8.6" cy="10" r="1.7" /><path d="M3.6 17.2 8.4 12.6a2 2 0 0 1 2.7-.08l4.2 3.7M14 14.6l2.1-1.9a2 2 0 0 1 2.7.04l1.7 1.6" /></>,
  videos: <><rect x="2.6" y="6" width="13" height="12" rx="3" /><path d="m15.6 12.6 4-2.9a.8.8 0 0 1 1.3.65v7.3a.8.8 0 0 1-1.3.65l-4-2.9Z" /></>,
  messages: <><path d="M12 4.2c4.8 0 8.3 3.1 8.3 7.1 0 4-3.5 7.1-8.3 7.1a10 10 0 0 1-2.4-.28l-4 1.5a.4.4 0 0 1-.53-.5l1-3a6.5 6.5 0 0 1-2.4-4.8c0-4 3.5-7.1 8.3-7.1Z" /></>,
  contacts: <><circle cx="12" cy="9" r="3.4" /><path d="M5.4 19.4a6.9 6.9 0 0 1 13.2 0" /></>,
  calls: <><path d="M6.6 3.9a1.6 1.6 0 0 1 2.3.5l1.3 2.2a1.6 1.6 0 0 1-.3 2l-1 .9a11 11 0 0 0 4.6 4.6l.9-1a1.6 1.6 0 0 1 2-.3l2.2 1.3a1.6 1.6 0 0 1 .5 2.3l-1 1.5a2.6 2.6 0 0 1-3 1C11.3 17.7 6.3 12.7 4.1 7.4a2.6 2.6 0 0 1 1-3Z" /></>,
  voicemail: <><circle cx="6.4" cy="13" r="3.5" /><circle cx="17.6" cy="13" r="3.5" /><path d="M6.4 16.5h11.2" /></>,
  mail: <><rect x="2.8" y="5.4" width="18.4" height="13.2" rx="3.2" /><path d="m4.2 8.4 6.5 4.6a2.2 2.2 0 0 0 2.6 0l6.5-4.6" /></>,
  notes: <><path d="M5.2 4.6h13.6v10.2l-4.4 4.6H5.2Z" /><path d="M18.6 14.8h-3.2a1 1 0 0 0-1 1v3.4" /><path d="M8.4 9h7.2M8.4 12.4h4.8" /></>,
  calendar: <><rect x="3.4" y="5" width="17.2" height="15" rx="3.2" /><path d="M3.4 9.6h17.2M8 3.4v3.2M16 3.4v3.2" /><circle cx="8.4" cy="13.6" r="1.1" fill="currentColor" stroke="none" /><circle cx="12" cy="13.6" r="1.1" fill="currentColor" stroke="none" /></>,
  reminders: <><circle cx="12" cy="12" r="8.4" /><path d="m8.4 12.2 2.4 2.4 4.8-4.9" /></>,
  voicememos: <><rect x="9" y="3.2" width="6" height="11" rx="3" /><path d="M5.6 11.6a6.4 6.4 0 0 0 12.8 0M12 18v2.8" /></>,
  health: <><path d="M12 19.6S4 15 4 9.9A4.3 4.3 0 0 1 12 7.6a4.3 4.3 0 0 1 8 2.3c0 5.1-8 9.7-8 9.7Z" /></>,
  safari: <><circle cx="12" cy="12" r="8.6" /><path d="m15.6 8.4-1.9 5.3-5.3 1.9 1.9-5.3Z" /></>,
  files: <><path d="M3.6 8.2V6.4a2 2 0 0 1 2-2h3.1a2 2 0 0 1 1.5.7l1 1.2a2 2 0 0 0 1.5.7h5.7a2 2 0 0 1 2 2v9.4a2 2 0 0 1-2 2H5.6a2 2 0 0 1-2-2Z" /></>,
  apps: <><rect x="3.6" y="3.6" width="7" height="7" rx="2.2" /><rect x="13.4" y="3.6" width="7" height="7" rx="2.2" /><rect x="3.6" y="13.4" width="7" height="7" rx="2.2" /><rect x="13.4" y="13.4" width="7" height="7" rx="2.2" /></>,
  music: <><path d="M9.4 17.4V6.6l9.2-2v10.8" /><circle cx="6.9" cy="17.4" r="2.5" /><circle cx="16.1" cy="15.4" r="2.5" /></>,
  wallet: <><rect x="3" y="5.6" width="18" height="12.8" rx="3.2" /><path d="M3 10.2h18" /><circle cx="16.8" cy="14.4" r="1.2" fill="currentColor" stroke="none" /></>,
  locations: <><path d="M12 21s6.6-6.1 6.6-10.6a6.6 6.6 0 1 0-13.2 0C5.4 14.9 12 21 12 21Z" /><circle cx="12" cy="10.2" r="2.4" /></>,
  device: <><rect x="6.6" y="2.6" width="10.8" height="18.8" rx="3" /><path d="M10.4 5.4h3.2" /><circle cx="12" cy="18" r="1" fill="currentColor" stroke="none" /></>,

  settings: <><circle cx="12" cy="12" r="3" /><path d="M19.2 14.4a1.6 1.6 0 0 0 .32 1.76l.06.06a1.94 1.94 0 1 1-2.74 2.74l-.06-.06a1.6 1.6 0 0 0-1.76-.32 1.6 1.6 0 0 0-.97 1.46v.17a1.94 1.94 0 1 1-3.88 0v-.09a1.6 1.6 0 0 0-1.05-1.46 1.6 1.6 0 0 0-1.76.32l-.06.06a1.94 1.94 0 1 1-2.74-2.74l.06-.06a1.6 1.6 0 0 0 .32-1.76 1.6 1.6 0 0 0-1.46-.97H3.3a1.94 1.94 0 1 1 0-3.88h.09a1.6 1.6 0 0 0 1.46-1.05 1.6 1.6 0 0 0-.32-1.76l-.06-.06a1.94 1.94 0 1 1 2.74-2.74l.06.06a1.6 1.6 0 0 0 1.76.32h.08a1.6 1.6 0 0 0 .97-1.46V3.3a1.94 1.94 0 0 1 3.88 0v.09a1.6 1.6 0 0 0 .97 1.46 1.6 1.6 0 0 0 1.76-.32l.06-.06a1.94 1.94 0 1 1 2.74 2.74l-.06.06a1.6 1.6 0 0 0-.32 1.76v.08a1.6 1.6 0 0 0 1.46.97h.17a1.94 1.94 0 0 1 0 3.88h-.09a1.6 1.6 0 0 0-1.46.97Z" /></>,
  sun: <><circle cx="12" cy="12" r="4.2" /><path d="M12 2.4v2.2M12 19.4v2.2M4.2 12H2M22 12h-2.2M6.5 6.5 4.9 4.9M19.1 19.1l-1.6-1.6M17.5 6.5l1.6-1.6M4.9 19.1l1.6-1.6" /></>,
  moon: <><path d="M20.4 14.6A8.6 8.6 0 0 1 9.4 3.6a8.6 8.6 0 1 0 11 11Z" /></>,
  chevron: <><path d="m9.4 5.6 6.4 6.4-6.4 6.4" /></>,
  search: <><circle cx="10.8" cy="10.8" r="6.6" /><path d="m15.7 15.7 4.1 4.1" /></>,
  import: <><path d="M12 3.6v11.2M7.8 10.8 12 15l4.2-4.2" /><path d="M4.4 15.6v2.4a2.4 2.4 0 0 0 2.4 2.4h10.4a2.4 2.4 0 0 0 2.4-2.4v-2.4" /></>,
  close: <><path d="m6.6 6.6 10.8 10.8M17.4 6.6 6.6 17.4" /></>,
  sidebar: <><rect x="3" y="4.6" width="18" height="14.8" rx="3.2" /><path d="M9.6 4.6v14.8" /></>,
  wrench: <><path d="M15.4 3.6a5 5 0 0 0-4.5 7.1L3.9 17.7a2 2 0 0 0 0 2.8 2 2 0 0 0 2.8 0l6.9-6.9a5 5 0 0 0 6.2-6.7l-2.9 2.9-2.6-.7-.7-2.6 2.9-2.9a5 5 0 0 0-1.1-.1Z" /></>,
};

interface IconProps {
  name: IconName;
  size?: number;
  className?: string;
}

export function Icon({ name, size = 19, className }: IconProps) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {P[name]}
    </svg>
  );
}
