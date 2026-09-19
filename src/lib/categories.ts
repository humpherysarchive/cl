/* The full set of data classes an iOS backup can yield. `group` mirrors the
   sectioned sidebar in Photos.app; order within a group is display order. */

export type CategoryId =
  | "photos"
  | "videos"
  | "messages"
  | "contacts"
  | "calls"
  | "voicemail"
  | "mail"
  | "notes"
  | "calendar"
  | "reminders"
  | "voicememos"
  | "health"
  | "safari"
  | "files"
  | "apps"
  | "music"
  | "wallet"
  | "locations"
  | "device";

export type GroupId = "library" | "communication" | "personal" | "device";

export interface Category {
  id: CategoryId;
  label: string;
  group: GroupId;
  /** One line shown in the empty state, in plain language. */
  blurb: string;
}

export const GROUPS: { id: GroupId; label: string }[] = [
  { id: "library", label: "Library" },
  { id: "communication", label: "Communication" },
  { id: "personal", label: "Personal" },
  { id: "device", label: "Device" },
];

export const CATEGORIES: Category[] = [
  { id: "photos", label: "Photos", group: "library", blurb: "Every picture from your camera roll and albums." },
  { id: "videos", label: "Videos", group: "library", blurb: "Recordings, slow-motion and time-lapse clips." },

  { id: "messages", label: "Messages", group: "communication", blurb: "Text and iMessage conversations, with attachments." },
  { id: "contacts", label: "Contacts", group: "communication", blurb: "Names, numbers, addresses and birthdays." },
  { id: "calls", label: "Call History", group: "communication", blurb: "Who called, when, and for how long." },
  { id: "voicemail", label: "Voicemail", group: "communication", blurb: "Saved voicemail recordings and transcripts." },
  { id: "mail", label: "Mail", group: "communication", blurb: "Mail accounts and locally stored messages." },

  { id: "notes", label: "Notes", group: "personal", blurb: "Written notes, checklists, sketches and scans." },
  { id: "calendar", label: "Calendar", group: "personal", blurb: "Events, invitations and recurring appointments." },
  { id: "reminders", label: "Reminders", group: "personal", blurb: "To-do lists and their due dates." },
  { id: "voicememos", label: "Voice Memos", group: "personal", blurb: "Audio recordings made on the device." },
  { id: "health", label: "Health", group: "personal", blurb: "Steps, workouts, sleep and medical records." },

  { id: "safari", label: "Safari", group: "device", blurb: "Bookmarks, reading list and browsing history." },
  { id: "files", label: "Files", group: "device", blurb: "Documents stored on the device and in iCloud Drive." },
  { id: "apps", label: "Apps", group: "device", blurb: "Installed apps and the data they kept locally." },
  { id: "music", label: "Music", group: "device", blurb: "Playlists, play counts and local audio." },
  { id: "wallet", label: "Wallet", group: "device", blurb: "Passes, tickets and loyalty cards." },
  { id: "locations", label: "Locations", group: "device", blurb: "Significant locations and saved places." },
  { id: "device", label: "Device Info", group: "device", blurb: "Model, iOS version, serial and backup details." },
];

export const CATEGORY_MAP: Record<CategoryId, Category> = Object.fromEntries(
  CATEGORIES.map((c) => [c.id, c]),
) as Record<CategoryId, Category>;

/** Shown on a first run, before the user has hidden anything. */
export const DEFAULT_VISIBLE: CategoryId[] = CATEGORIES.map((c) => c.id);
