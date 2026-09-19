import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listPhotos, thumbUrl, type Photo } from "../lib/backup";
import { useSession } from "../lib/session";
import { Icon } from "../components/Icon";
import { Lightbox } from "../components/Lightbox";
import "./PhotosView.css";

/* Layout constants. Row heights have to be known up front — that is what makes
   the windowing below a lookup rather than a measurement pass. */
const MIN_CELL = 116;
const GAP = 4;
const HEADER_H = 46;
const GUTTER = 24;
/** Rows rendered beyond the viewport, so scrolling does not reveal gaps. */
const OVERSCAN = 3;

type Row =
  | { kind: "header"; key: string; label: string; count: number }
  | { kind: "photos"; key: string; items: Photo[] };

interface Positioned {
  row: Row;
  top: number;
  height: number;
}

export function PhotosView({ onImport }: { onImport: () => void }) {
  const { device } = useSession();
  const [photos, setPhotos] = useState<Photo[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [viewing, setViewing] = useState<number | null>(null);

  const scrollRef = useRef<HTMLDivElement>(null);
  const [viewport, setViewport] = useState({ width: 0, height: 0, scrollTop: 0 });

  useEffect(() => {
    if (!device) {
      setPhotos(null);
      return;
    }
    let cancelled = false;
    setError(null);
    setPhotos(null);
    listPhotos()
      .then((p) => !cancelled && setPhotos(p))
      .catch((e) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [device]);

  // Track size and scroll position. ResizeObserver rather than a window
  // listener, because the sidebar collapsing changes our width without the
  // window changing at all.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const measure = () =>
      setViewport({
        width: el.clientWidth,
        height: el.clientHeight,
        scrollTop: el.scrollTop,
      });
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [photos]);

  const onScroll = useCallback(() => {
    const el = scrollRef.current;
    if (el) setViewport((v) => ({ ...v, scrollTop: el.scrollTop }));
  }, []);

  const columns = Math.max(
    1,
    Math.floor((viewport.width - GUTTER * 2 + GAP) / (MIN_CELL + GAP)),
  );
  const cell = columns
    ? Math.floor((viewport.width - GUTTER * 2 - GAP * (columns - 1)) / columns)
    : MIN_CELL;

  /* Group by capture day and lay the rows out. Recomputed only when the photo
     list or the column count changes, not on every scroll frame. */
  const { rows, total } = useMemo(() => {
    if (!photos || columns < 1) return { rows: [] as Positioned[], total: 0 };

    const groups: { label: string; items: Photo[] }[] = [];
    let currentKey = "";
    for (const photo of photos) {
      const key = dayKey(photo.created);
      if (key !== currentKey) {
        groups.push({ label: dayLabel(photo.created), items: [] });
        currentKey = key;
      }
      groups[groups.length - 1].items.push(photo);
    }

    const out: Positioned[] = [];
    let top = 0;
    for (const group of groups) {
      out.push({
        row: {
          kind: "header",
          key: `h:${group.label}:${top}`,
          label: group.label,
          count: group.items.length,
        },
        top,
        height: HEADER_H,
      });
      top += HEADER_H;

      for (let i = 0; i < group.items.length; i += columns) {
        const items = group.items.slice(i, i + columns);
        out.push({
          row: { kind: "photos", key: `r:${items[0].id}`, items },
          top,
          height: cell + GAP,
        });
        top += cell + GAP;
      }
    }
    return { rows: out, total: top };
  }, [photos, columns, cell]);

  // Binary search for the first row at or past the top of the viewport.
  const visible = useMemo(() => {
    if (rows.length === 0) return [];
    const start = viewport.scrollTop;
    const end = start + viewport.height;

    let lo = 0;
    let hi = rows.length - 1;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (rows[mid].top + rows[mid].height <= start) lo = mid + 1;
      else hi = mid;
    }
    const from = Math.max(0, lo - OVERSCAN);

    let to = from;
    while (to < rows.length && rows[to].top < end + OVERSCAN * (cell + GAP)) to += 1;
    return rows.slice(from, to);
  }, [rows, viewport.scrollTop, viewport.height, cell]);

  const openAt = useCallback(
    (photo: Photo) => {
      const index = photos?.findIndex((p) => p.id === photo.id) ?? -1;
      if (index >= 0) setViewing(index);
    },
    [photos],
  );

  if (!device) {
    return <Empty onImport={onImport} />;
  }

  return (
    <div className="photos">
      <header className="photos__header">
        <h1 className="photos__title">Photos</h1>
        <p className="photos__subtitle">
          {photos === null && !error && "Reading the camera roll…"}
          {error && "Could not read the camera roll."}
          {photos !== null &&
            (photos.length === 0
              ? "No photos in this backup."
              : `${photos.length.toLocaleString()} photos`)}
        </p>
      </header>

      {error && (
        <p className="photos__error" role="alert">
          {error}
        </p>
      )}

      <div className="photos__scroll scroll" ref={scrollRef} onScroll={onScroll}>
        {/* One tall spacer holds the scrollbar; only visible rows are mounted. */}
        <div className="photos__canvas" style={{ height: total }}>
          {visible.map(({ row, top, height }) =>
            row.kind === "header" ? (
              <div className="photos__day" key={row.key} style={{ top, height }}>
                <span>{row.label}</span>
                <small>{row.count}</small>
              </div>
            ) : (
              <div
                className="photos__row"
                key={row.key}
                style={{ top, height, gap: GAP, paddingInline: GUTTER }}
              >
                {row.items.map((photo) => (
                  <button
                    type="button"
                    className="cell"
                    key={photo.id}
                    style={{ width: cell, height: cell }}
                    onClick={() => openAt(photo)}
                    aria-label={photo.filename}
                  >
                    <img
                      src={thumbUrl(photo.id)}
                      alt=""
                      loading="lazy"
                      decoding="async"
                      width={cell}
                      height={cell}
                      onError={(e) => {
                        // A format we cannot decode: leave the placeholder
                        // rather than a broken-image icon.
                        e.currentTarget.style.visibility = "hidden";
                      }}
                    />
                  </button>
                ))}
              </div>
            ),
          )}
        </div>
      </div>

      {viewing !== null && photos && (
        <Lightbox
          photos={photos}
          index={viewing}
          onIndex={setViewing}
          onClose={() => setViewing(null)}
        />
      )}
    </div>
  );
}

function Empty({ onImport }: { onImport: () => void }) {
  return (
    <article className="view">
      <header className="view__header">
        <h1 className="view__title">Photos</h1>
        <p className="view__subtitle">Every picture from your camera roll and albums.</p>
      </header>
      <div className="view__body">
        <div className="empty">
          <div className="empty__glyph">
            <Icon name="photos" size={30} />
          </div>
          <h2 className="empty__title">Nothing imported yet</h2>
          <p className="empty__text">
            Connect an iPhone or point the app at a backup folder, and your photos
            will show up here.
          </p>
          <button type="button" className="btn btn--primary" onClick={onImport}>
            <Icon name="import" size={16} />
            Import from device or backup
          </button>
        </div>
      </div>
    </article>
  );
}

/* ---- Date grouping ----------------------------------------------------- */

function dayKey(created: number | null): string {
  if (created === null) return "undated";
  const d = new Date(created * 1000);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
}

const DAY_FORMAT = new Intl.DateTimeFormat(undefined, {
  weekday: "long",
  day: "numeric",
  month: "long",
  year: "numeric",
});

function dayLabel(created: number | null): string {
  if (created === null) return "Date unknown";
  return DAY_FORMAT.format(new Date(created * 1000));
}
