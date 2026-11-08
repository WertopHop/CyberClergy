// Speech card floating above a cleric. Purely presentational: expiry, typing
// and queueing are owned by `useChapel`.
export function PrayerBubble({
  text,
  meta,
  quiet = false,
  typing = false,
}: {
  text: string;
  /** Counter, theme or source label shown under the prayer. */
  meta?: string | null;
  /** A prayer from the tradition's canon rather than a network. */
  quiet?: boolean;
  /** The text is still being revealed, so the cursor keeps blinking. */
  typing?: boolean;
}) {
  return (
    <div className={`bubble ${quiet ? "bubble--quiet" : ""}`} role="note">
      <p className="bubble__text">
        {text}
        {typing ? (
          <span className="bubble__cursor" aria-hidden="true" />
        ) : null}
      </p>
      {meta ? <p className="bubble__meta">{meta}</p> : null}
    </div>
  );
}
