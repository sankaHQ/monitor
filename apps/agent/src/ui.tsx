export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function Tick({ pass }: { pass: boolean | null }) {
  if (pass === true) return <span className="tick pass">✓</span>;
  if (pass === false) return <span className="tick fail">✕</span>;
  return <span className="tick unknown">–</span>;
}
