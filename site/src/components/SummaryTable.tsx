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
                      className={record.nanos === best ? "best" : undefined}
                    >
                      {record.nanos.toFixed(2)}
                    </td>
                  );
                })}
              </tr>
            );
          })}
        </tbody>
        <caption>
          Nanoseconds per operation, median of criterion's samples. ★ marks the
          fastest decimal. Lower is better.
        </caption>
      </table>
    </div>
  );
}
