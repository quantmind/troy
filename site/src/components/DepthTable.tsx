import { bookDepths, presentBook, sweepSeries, type Snapshot } from "@/lib/bench";

/**
 * One row per book operation, one column per depth, and the ratio between the
 * ends of the sweep. The ratio is the column to read: an operation that does
 * not care how deep the book is sits near 1, and one that shifts the levels
 * below it does not.
 */
export default function DepthTable({ snapshot }: { snapshot: Snapshot }) {
  const depths = bookDepths(snapshot);
  const shallowest = depths[0];
  const deepest = depths[depths.length - 1];

  return (
    <div className="scroll">
      <table>
        <caption>
          <strong>Nanoseconds per operation</strong> against the depth of one
          side, median of criterion's samples. The last column is the cost at {deepest} levels
          over the cost at {shallowest}; ★ marks the operations that stay flat.
        </caption>
        <thead>
          <tr>
            <th>operation</th>
            {depths.map((depth) => (
              <th key={depth}>{depth}</th>
            ))}
            <th>
              {deepest} / {shallowest}
            </th>
          </tr>
        </thead>
        <tbody>
          {presentBook(snapshot).map(({ operation }) => {
            const curve = sweepSeries(operation, snapshot).get("Dec") ?? [];
            const byDepth = new Map(curve);
            const ends = [shallowest, deepest].map((depth) =>
              depth === undefined ? undefined : byDepth.get(depth),
            );
            const ratio =
              ends[0] && ends[1] ? (ends[1] / ends[0]).toFixed(1) : null;

            return (
              <tr key={operation}>
                <th>{operation.replace(/^book_/, "")}</th>
                {depths.map((depth) => {
                  const value = byDepth.get(depth);
                  return value === undefined ? (
                    <td className="none" key={depth}>
                      —
                    </td>
                  ) : (
                    <td key={depth}>{value.toFixed(2)}</td>
                  );
                })}
                <td className={ratio && Number(ratio) < 1.5 ? "best" : undefined}>
                  {ratio ? `${ratio}×` : "—"}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
