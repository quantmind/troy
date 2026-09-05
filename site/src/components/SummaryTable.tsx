import {
  at,
  fastestDecimal,
  IMPLEMENTATIONS,
  present,
  timings,
  type Snapshot,
} from "@/lib/bench";

/** Every per-operation card as one table. */
export default function SummaryTable({ snapshot }: { snapshot: Snapshot }) {
  const table = timings(snapshot);

  return (
    <div className="scroll">
      <table>
        <caption>
          <strong>Nanoseconds per operation</strong>, median of criterion's
          samples, coloured by rank within each row: green fastest, then blue,
          amber, red. ★ marks the fastest decimal. Lower is better.
        </caption>
        <thead>
          <tr>
            <th>operation</th>
            {IMPLEMENTATIONS.map((name) => (
              <th key={name}>{name}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {present(snapshot).map(({ operation }) => {
            const best = fastestDecimal(table, operation);
            // rank only what the row actually measured, so a row with two
            // entries ranks one and two rather than reserving the slow colours
            const rank = new Map(
              IMPLEMENTATIONS.flatMap((name) => {
                const record = at(table, operation, name);
                return record ? [[name, record.nanos] as const] : [];
              })
                .sort(([, a], [, b]) => a - b)
                .map(([name], index) => [name, index + 1]),
            );
            return (
              <tr key={operation}>
                <th>{operation}</th>
                {IMPLEMENTATIONS.map((name) => {
                  const record = at(table, operation, name);
                  if (!record) {
                    return (
                      <td className="none" key={name}>
                        —
                      </td>
                    );
                  }
                  return (
                    <td
                      key={name}
                      className={[
                        `rank-${rank.get(name)}`,
                        record.nanos === best ? "best" : null,
                      ]
                        .filter(Boolean)
                        .join(" ")}
                    >
                      {record.nanos.toFixed(2)}
                    </td>
                  );
                })}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
