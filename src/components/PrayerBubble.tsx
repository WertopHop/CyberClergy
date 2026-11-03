// Speech card floating above a cleric. Purely presentational: expiry and
// rotation are owned by `useChapel`.
export function PrayerBubble({ text }: { text: string }) {
  return (
    <div className="bubble" role="note">
      <p className="bubble__text">{text}</p>
    </div>
  );
}
