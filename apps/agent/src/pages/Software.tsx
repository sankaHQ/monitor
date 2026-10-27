import { useState } from "react";
import type { AppContext } from "../context";
import { PageTitle, Segmented } from "../ui";

type Filter = "all" | "flagged";

/// Installed programs, searchable, with a filter for those past or near the
/// end of vendor support.
export function SoftwarePage({ ctx }: { ctx: AppContext }) {
  const { s, scan } = ctx;
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  if (!scan) {
    return (
      <>
        <PageTitle icon="software">{s.nav.software}</PageTitle>
        <p className="hint">{s.software.empty}</p>
      </>
    );
  }
  const flagged = scan.software.filter((item) => item.flagged);
  const needle = query.trim().toLowerCase();
  const rows = (filter === "flagged" ? flagged : scan.software).filter(
    (item) =>
      !needle ||
      item.name.toLowerCase().includes(needle) ||
      (item.publisher ?? "").toLowerCase().includes(needle),
  );

  return (
    <>
      <PageTitle icon="software">{s.nav.software}</PageTitle>
      <div className="toolbar">
        <input
          className="search"
          type="search"
          value={query}
          placeholder={s.software.search(scan.software.length)}
          aria-label={s.software.search(scan.software.length)}
          onChange={(event) => setQuery(event.target.value)}
        />
      </div>
      <div className="toolbar">
        <Segmented
          value={filter}
          label={s.nav.software}
          options={[
            { value: "all", label: s.software.all(scan.software.length) },
            { value: "flagged", label: s.software.flagged(flagged.length) },
          ]}
          onChange={setFilter}
        />
      </div>
      <div className="list">
        {rows.length === 0 ? (
          <div className="empty">{filter === "flagged" ? s.software.nothingFlagged : s.software.nothing}</div>
        ) : (
          rows.map((item, index) => (
            <div className="row" key={`${item.name}-${item.version}-${index}`}>
              <div className="text">
                <b>{item.name}</b>
                <span>
                  {[item.version, item.publisher].filter(Boolean).join(" · ")}
                  {item.flagged && item.successor ? ` · ${s.software.replaceWith(item.successor)}` : ""}
                </span>
              </div>
              <span className={item.flagged ? "aside bad" : "aside"}>
                {item.flagged && item.supportEndsOn
                  ? item.status === "ended"
                    ? s.software.supportEnded(item.supportEndsOn)
                    : s.software.supportEnds(item.supportEndsOn)
                  : item.statusLabel}
              </span>
            </div>
          ))
        )}
      </div>
      <p className="hint">{s.software.footer(scan.catalogVerifiedOn ?? "—")}</p>
    </>
  );
}
