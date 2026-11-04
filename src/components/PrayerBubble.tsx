// Speech card floating above a cleric. Purely presentational: expiry and
// rotation are owned by `useChapel`.
export function PrayerBubble({
  text,
  meta,
  quiet = false,
}: {
  text: string;
  /** Counter, theme or source label shown under the prayer. */
  meta?: string | null;
  /** A prayer from the tradition's canon rather than a network. */
  quiet?: boolean;
}) {
  return (
    <div className={`bubble ${quiet ? "bubble--quiet" : ""}`} role="note">
      <p className="bubble__text">{text}</p>
      {meta ? <p className="bubble__meta">{meta}</p> : null}
    </div>
  );
}
