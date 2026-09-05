import {
  at,
  COLOR,
  fastestDecimal,
  IMPLEMENTATIONS,
  nanos,
  type Table,
} from "@/lib/bench";

interface Props {
  operation: string;
  caption: string;
  table: Table;
}

/**
 * One operation, every implementation, scaled to its own slowest bar. A card
 * shows who wins that operation and by how much, not how operations compare
 * with each other.
 */
export default function BarCard({ operation, caption, table }: Props) {
  const records = IMPLEMENTATIONS.map(
    (name) => [name, at(table, operation, name)] as const,
  );
  const slowest = Math.max(
    ...records.map(([, record]) => record?.nanos ?? 0),
  );
  const best = fastestDecimal(table, operation);

  return (
    <div className="card">
      <h3>{operation}</h3>
      <p className="caption">{caption}</p>
      <div className="bars">
        {records.map(([name, record]) => {
          if (!record) {
            // f64 has no arm in the conversion groups because converting it to
            // itself measures nothing; anywhere else the operation is missing
            const missing =
              name === "f64" ? "not applicable" : "not implemented";
            return (
              <div className="bar-row" key={name}>
                <div className="bar-name">{name}</div>
                <div className="absent">{missing}</div>
                <div />
              </div>
            );
          }

          const width = Math.max((record.nanos / slowest) * 100, 1.2);
          const note = record.nanos === best ? " · fastest decimal" : "";
          const tip =
            `<b>${name} — ${operation}</b>${nanos(record.nanos)} per operation${note}<br>` +
            `<span>95% CI ${nanos(record.low)} – ${nanos(record.high)}</span>`;

          return (
            <div className="bar-row" key={name} data-tip={tip}>
              <div className="bar-name">{name}</div>
              <div className="track">
                <div
                  className="bar"
                  style={{
                    width: `${width.toFixed(1)}%`,
                    background: COLOR[name],
                  }}
                />
              </div>
              <div className="bar-value">{nanos(record.nanos)}</div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
