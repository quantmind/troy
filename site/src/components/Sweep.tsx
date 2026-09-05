import { COLOR, nanos, type Curve } from "@/lib/bench";

interface Props {
  /** One curve per series name, each sorted by parameter. */
  series: Map<string, Curve>;
  /** Series drawn in this order, so the legend and the lines agree. */
  order?: string[];
  axis: string;
  label: string;
  /**
   * The digit sweeps step by one, so the axis is linear and the 18-to-19 step
   * reads as the single digit it is. Depth quadruples, so it is drawn on a log
   * axis: on a linear one the first three depths would sit on top of each
   * other and a flat operation would be indistinguishable from a linear one.
   */
  scale?: "linear" | "log";
  /** Name each line at its own end, so identity never rests on colour alone. */
  tags?: boolean;
  width?: number;
  height?: number;
}

/**
 * Round tick values, four or so of them, running past the data to the next one
 * up. The axis takes its ceiling from that top tick rather than from the data,
 * so a flat line is drawn inside the plot instead of pinned along its top edge
 * where the curve and the frame become the same stroke.
 */
function axisFor(largest: number): {
  values: number[];
  step: number;
  ceiling: number;
} {
  const raw = largest / 4;
  const magnitude = 10 ** Math.floor(Math.log10(raw));
  const normalised = raw / magnitude;
  const step =
    (normalised <= 1 ? 1 : normalised <= 2 ? 2 : normalised <= 5 ? 5 : 10) *
    magnitude;

  const count = Math.ceil(largest / step);
  // counted rather than accumulated, so a fractional step does not drift
  const values = Array.from({ length: count + 1 }, (_, index) => index * step);
  return { values, step, ceiling: count * step };
}

export default function Sweep({
  series,
  order,
  axis,
  label,
  scale = "linear",
  tags = true,
  width = 720,
  height = 300,
}: Props) {
  const names = (order ?? [...series.keys()]).filter((name) =>
    series.get(name)?.length,
  );
  const points = names.flatMap((name) => series.get(name)!);
  const parameters = [...new Set(points.map(([p]) => p))].sort((a, b) => a - b);
  const {
    values: ticks,
    step,
    ceiling,
  } = axisFor(Math.max(...points.map(([, n]) => n)));

  // the top margin carries the unit: the highest tick is the ceiling by
  // construction, so its label sits on the top gridline and the unit needs a
  // line of its own above it rather than the couple of pixels a tickless top
  // would have left
  const [left, right, top, bottom] = [52, 16, 28, 42];
  const plotW = width - left - right;
  const plotH = height - top - bottom;

  const project = scale === "log" ? Math.log2 : (value: number) => value;
  const first = project(parameters[0]!);
  const span = project(parameters[parameters.length - 1]!) - first || 1;
  const xOf = (parameter: number) =>
    left + ((project(parameter) - first) / span) * plotW;
  const yOf = (value: number) => top + plotH - (value / ceiling) * plotH;

  const places = step < 1 ? Math.min(3, Math.ceil(-Math.log10(step))) : 0;

  return (
    <svg
      viewBox={`0 0 ${width} ${height}`}
      className="sweep"
      role="img"
      aria-label={label}
    >
      {ticks.map((tick) => (
        <g key={tick}>
          <line
            x1={left}
            y1={yOf(tick)}
            x2={width - right}
            y2={yOf(tick)}
            className="gridline"
          />
          <text x={left - 8} y={yOf(tick) + 4} className="tick end">
            {tick.toFixed(places)}
          </text>
        </g>
      ))}
      <text x={left - 8} y={top - 10} className="tick end">
        ns
      </text>

      {parameters.map((parameter) => (
        <text
          key={parameter}
          x={xOf(parameter)}
          y={height - bottom + 18}
          className="tick mid"
        >
          {parameter}
        </text>
      ))}
      <text x={left + plotW / 2} y={height - 6} className="axis mid">
        {axis}
      </text>

      {names.map((name) => {
        const curve = series.get(name)!;
        const path = curve
          .map(
            ([p, n], index) =>
              `${index === 0 ? "M" : "L"}${xOf(p).toFixed(1)},${yOf(n).toFixed(1)}`,
          )
          .join(" ");
        const [lastP, lastN] = curve[curve.length - 1]!;
        const color = COLOR[name] ?? "var(--dec)";

        return (
          <g key={name}>
            <path
              d={path}
              fill="none"
              stroke={color}
              strokeWidth={2}
              strokeLinejoin="round"
            />
            {curve.map(([parameter, value]) => (
              <circle
                key={parameter}
                cx={xOf(parameter)}
                cy={yOf(value)}
                r={4.5}
                fill={color}
                className="dot"
              >
                <title>{`${name} at ${parameter}: ${nanos(value)}`}</title>
              </circle>
            ))}
            {tags && (
              <text
                x={xOf(lastP) - 6}
                y={yOf(lastN) - 10}
                className="tag end"
                fill={color}
              >
                {name}
              </text>
            )}
          </g>
        );
      })}
    </svg>
  );
}
