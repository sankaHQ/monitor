import type { ReactNode } from "react";

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

const ICONS = {
  overview: (
    <>
      <rect x="2.5" y="2.5" width="4.5" height="4.5" rx="1" />
      <rect x="9" y="2.5" width="4.5" height="4.5" rx="1" />
      <rect x="2.5" y="9" width="4.5" height="4.5" rx="1" />
      <rect x="9" y="9" width="4.5" height="4.5" rx="1" />
    </>
  ),
  risks: (
    <>
      <path d="M8 2.5l6 10.5H2z" />
      <path d="M8 6.5v3M8 11.3v.2" />
    </>
  ),
  software: (
    <>
      <rect x="2.5" y="2.5" width="11" height="11" rx="2" />
      <path d="M2.5 6h11" />
    </>
  ),
  saas: <path d="M4.5 12.5a3 3 0 0 1-.3-6 4 4 0 0 1 7.6-.8 2.9 2.9 0 0 1 .2 5.8z" />,
  reports: (
    <>
      <path d="M4 2.5h5l3 3v8H4z" />
      <path d="M9 2.5v3h3M6 9h4M6 11h3" />
    </>
  ),
  workspace: (
    <>
      <path d="M6.5 9.5l3-3" />
      <path d="M7.5 4.5l1-1a2.5 2.5 0 0 1 3.5 3.5l-1 1M8.5 11.5l-1 1A2.5 2.5 0 0 1 4 9l1-1" />
    </>
  ),
  privacy: (
    <>
      <path d="M1.8 8s2.3-4.5 6.2-4.5S14.2 8 14.2 8s-2.3 4.5-6.2 4.5S1.8 8 1.8 8z" />
      <circle cx="8" cy="8" r="2" />
    </>
  ),
  settings: (
    <>
      <path d="M3 4.5h10M3 8h10M3 11.5h10" />
      <circle cx="6" cy="4.5" r="1.4" />
      <circle cx="10.5" cy="8" r="1.4" />
      <circle cx="5" cy="11.5" r="1.4" />
    </>
  ),
  refresh: (
    <>
      <path d="M13 8a5 5 0 1 1-1.5-3.6" />
      <path d="M13 2.8v2.6h-2.6" />
    </>
  ),
  arrow: <path d="M5 8h6M8.5 5.5L11 8l-2.5 2.5" />,
  lock: (
    <>
      <rect x="3.5" y="7" width="9" height="6.5" rx="1.4" />
      <path d="M5.5 7V5.2a2.5 2.5 0 0 1 5 0V7" />
    </>
  ),
  // Security checks, by PostureCheckId.
  disk_encryption: (
    <>
      <rect x="3" y="7" width="10" height="7" rx="1.5" />
      <path d="M5.5 7V5a2.5 2.5 0 0 1 5 0v2" />
    </>
  ),
  firewall: (
    <>
      <rect x="2.5" y="3" width="11" height="10" rx="1" />
      <path d="M2.5 6.3h11M2.5 9.7h11M6 3v3.3M10 6.3v3.4M6 9.7V13" />
    </>
  ),
  gatekeeper: (
    <>
      <path d="M3 13.5V6l5-3 5 3v7.5" />
      <path d="M6 13.5V9.5h4v4" />
    </>
  ),
  screen_lock: (
    <>
      <rect x="2" y="3" width="12" height="8" rx="1.2" />
      <path d="M6 14h4M8 11v3" />
    </>
  ),
  os_update: (
    <>
      <circle cx="8" cy="8" r="5.5" />
      <path d="M8 5v5.2M5.8 8l2.2 2.2L10.2 8" />
    </>
  ),
  automatic_updates: (
    <>
      <path d="M13 8a5 5 0 1 1-1.5-3.6" />
      <path d="M13 2.8v2.6h-2.6" />
    </>
  ),
  antivirus: (
    <>
      <path d="M8 2.5l5 2v3.5c0 3-2.2 5-5 5.5-2.8-.5-5-2.5-5-5.5V4.5z" />
      <path d="M6 8l1.5 1.5L10.2 7" />
    </>
  ),
  local_admin: (
    <>
      <circle cx="8" cy="5.5" r="2.5" />
      <path d="M3 13.5c.6-2.5 2.6-4 5-4s4.4 1.5 5 4" />
    </>
  ),
  device_management: (
    <>
      <rect x="3" y="2.5" width="10" height="11" rx="1" />
      <path d="M6 5.5h1M9 5.5h1M6 8h1M9 8h1M7 13.5V11h2v2.5" />
    </>
  ),
  secure_boot: (
    <>
      <path d="M8 2v5" />
      <path d="M4.6 4.4a5 5 0 1 0 6.8 0" />
    </>
  ),
  system_integrity: (
    <>
      <rect x="4" y="4" width="8" height="8" rx="1" />
      <path d="M6 2v2M10 2v2M6 12v2M10 12v2M2 6h2M2 10h2M12 6h2M12 10h2" />
    </>
  ),
} as const;

export type IconName = keyof typeof ICONS;

export function Icon({ name }: { name: string }) {
  const paths = ICONS[name as IconName] ?? ICONS.overview;
  return (
    <svg
      className="icon"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {paths}
    </svg>
  );
}

/// A check's result: green check, red mark, or gray when the check can't tell.
export function StatusMark({ pass, label }: { pass: boolean | null; label: string }) {
  const kind = pass === true ? "pass" : pass === false ? "fail" : "unknown";
  return <span className={`status ${kind}`} role="img" aria-label={label} />;
}

export function Switch({
  checked,
  label,
  disabled,
  onChange,
}: {
  checked: boolean;
  label: string;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <button
      type="button"
      className="switch"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
    />
  );
}

export function Segmented<T extends string>({
  value,
  options,
  label,
  disabled,
  onChange,
}: {
  value: T;
  options: readonly { value: T; label: string }[];
  label: string;
  disabled?: boolean;
  onChange: (value: T) => void;
}) {
  return (
    <span className="segmented" role="group" aria-label={label}>
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          className={value === option.value ? "on" : undefined}
          aria-pressed={value === option.value}
          disabled={disabled}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </span>
  );
}

/// One row of a grouped list: a title, an optional line under it, and
/// whatever control or value sits on the right.
export function Row({
  title,
  detail,
  children,
  top,
}: {
  title: ReactNode;
  detail?: ReactNode;
  children?: ReactNode;
  top?: boolean;
}) {
  return (
    <div className={top ? "row top" : "row"}>
      <div className="text">
        <b>{title}</b>
        {detail ? <span>{detail}</span> : null}
      </div>
      {children}
    </div>
  );
}

export function PageTitle({ icon, children }: { icon: string; children: ReactNode }) {
  return (
    <h1 className="page-title">
      <Icon name={icon} />
      {children}
    </h1>
  );
}
