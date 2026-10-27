import lens from "../assets/lens.svg";
import type { AppContext, Screen } from "../context";
import { StatusMark } from "../ui";

/// The first launch: what the app does, the scan running (it starts at once,
/// as before), and the way to a company connection.
export function FirstRun({ ctx, onDone }: { ctx: AppContext; onDone: (next: Screen) => void }) {
  const { s, scan, scanning } = ctx;
  const done = scan !== null && !scanning;
  return (
    <div className="first">
      <img src={lens} alt="" />
      <h1>{s.first.title}</h1>
      <p>{s.first.body}</p>
      <p>{s.first.nothingLeaves}</p>
      <div className="list">
        {s.first.steps.map((step) => (
          <div className="row" key={step}>
            <div className="text">
              <b>{step}</b>
            </div>
            {done ? <StatusMark pass label={step} /> : <span className="aside">…</span>}
          </div>
        ))}
      </div>
      <div className="actions">
        <button className="button primary" disabled={!done} onClick={() => onDone("overview")}>
          {s.first.seeResults}
        </button>
        <button className="link accent" onClick={() => onDone("workspace")}>
          {s.first.connect}
        </button>
      </div>
    </div>
  );
}
