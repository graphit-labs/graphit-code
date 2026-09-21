import { Download, X } from "lucide-react";

interface TabularResultsProps {
  columns: string[];
  rows: unknown[][];
  onClose: () => void;
}

function downloadCSV(columns: string[], rows: unknown[][]) {
  const escape = (v: unknown) => {
    const s =
      typeof v === "object" && v !== null ? JSON.stringify(v) : String(v ?? "");
    return `"${s.replace(/"/g, '""')}"`;
  };
  const header = columns.map(escape).join(",");
  const body = rows.map((r) => r.map(escape).join(",")).join("\n");
  const blob = new Blob([header + "\n" + body], {
    type: "text/csv;charset=utf-8;",
  });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `query-results-${Date.now()}.csv`;
  a.click();
  URL.revokeObjectURL(url);
}

export function TabularResults({
  columns,
  rows,
  onClose,
}: TabularResultsProps) {
  return (
    <section className="work-panel">
      <header className="work-actions justify-between mb-4">
        <h3 className="text-sm font-semibold">{rows.length} result rows</h3>
        <div className="work-actions">
          <button
            className="work-button"
            onClick={() => downloadCSV(columns, rows)}
          >
            Export CSV
          </button>
          <button className="work-button" onClick={onClose}>
            Close results
          </button>
        </div>
      </header>
      <div className="work-table-wrap">
        <table className="work-table">
          <thead>
            <tr>
              {columns.map((c, i) => (
                <th key={i}>{c}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((row, i) => (
              <tr key={i}>
                {row.map((cell, j) => (
                  <td key={j}>
                    <code>
                      {typeof cell === "object" && cell !== null
                        ? JSON.stringify(cell)
                        : String(cell ?? "")}
                    </code>
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}
