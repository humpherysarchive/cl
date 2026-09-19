import { useEffect, useRef, useState } from "react";
import { fullUrl, type Photo } from "../lib/backup";
import { Icon } from "./Icon";
import "./Lightbox.css";

interface LightboxProps {
  photos: Photo[];
  index: number;
  onIndex: (next: number) => void;
  onClose: () => void;
}

export function Lightbox({ photos, index, onIndex, onClose }: LightboxProps) {
  const photo = photos[index];
  const [loading, setLoading] = useState(true);
  const [failed, setFailed] = useState(false);
  const closeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    setLoading(true);
    setFailed(false);
  }, [photo?.id]);

  useEffect(() => {
    closeRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      if (e.key === "ArrowRight" && index < photos.length - 1) onIndex(index + 1);
      if (e.key === "ArrowLeft" && index > 0) onIndex(index - 1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [index, photos.length, onIndex, onClose]);

  if (!photo) return null;

  return (
    <div className="lightbox" role="dialog" aria-modal="true" aria-label={photo.filename}>
      <button
        type="button"
        className="lightbox__backdrop"
        aria-label="Close"
        onClick={onClose}
      />

      <header className="lightbox__bar">
        <div className="lightbox__meta">
          <span className="lightbox__name">{photo.filename}</span>
          <span className="lightbox__detail">
            {[
              photo.created !== null && formatWhen(photo.created),
              photo.width && photo.height && `${photo.width} × ${photo.height}`,
              formatBytes(photo.size),
            ]
              .filter(Boolean)
              .join(" · ")}
          </span>
        </div>
        <button
          type="button"
          className="lightbox__close"
          onClick={onClose}
          aria-label="Close"
          ref={closeRef}
        >
          <Icon name="close" size={17} />
        </button>
      </header>

      <div className="lightbox__stage">
        {loading && !failed && <div className="spinner" aria-label="Loading" />}
        {failed ? (
          <p className="lightbox__failed">
            This image could not be decoded. The original is still in the backup
            and can be exported.
          </p>
        ) : (
          <img
            /* Keyed so switching photos remounts rather than showing the
               previous image while the next one decodes. */
            key={photo.id}
            src={fullUrl(photo.id)}
            alt={photo.filename}
            onLoad={() => setLoading(false)}
            onError={() => {
              setLoading(false);
              setFailed(true);
            }}
            style={{ visibility: loading ? "hidden" : "visible" }}
          />
        )}
      </div>

      <button
        type="button"
        className="lightbox__nav lightbox__nav--prev"
        onClick={() => onIndex(index - 1)}
        disabled={index === 0}
        aria-label="Previous photo"
      >
        <Icon name="chevron" size={22} />
      </button>
      <button
        type="button"
        className="lightbox__nav lightbox__nav--next"
        onClick={() => onIndex(index + 1)}
        disabled={index >= photos.length - 1}
        aria-label="Next photo"
      >
        <Icon name="chevron" size={22} />
      </button>
    </div>
  );
}

const WHEN_FORMAT = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "short",
});

function formatWhen(seconds: number) {
  return WHEN_FORMAT.format(new Date(seconds * 1000));
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
