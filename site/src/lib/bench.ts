/**
 * The benchmark snapshot, and the presentation metadata the pages read it
 * through.
 *
 * `.dev/bench-report save` owns collection: it reads criterion's JSON and
 * writes docs/bench-data.json, one record per (operation, implementation,
 * parameter) in nanoseconds for a single operation. This module owns nothing
 * but how those records are ordered and described on the web.
 */
import raw from "../../../docs/bench-data.json";

export interface Machine {
  cpu?: string;
  arch?: string;
  rustc?: string;
  commit?: string;
  measured?: string;
}

export interface Measurement {
  operation: string;
  implementation: string;
  parameter: string | null;
  count: number;
  nanos: number;
  low: number;
  high: number;
}

export interface Snapshot {
  machine: Machine;
  count: number;
  results: Measurement[];
}

export const snapshot = raw as Snapshot;

/** f64 leads as the inexact reference the decimals are measured against. */
export const IMPLEMENTATIONS = ["f64", "Dec", "rust_decimal", "fastnum"];
export const DECIMALS = ["Dec", "rust_decimal", "fastnum"];

// f64 is drawn in neutral ink rather than a categorical hue: it is the
// reference, not a fourth competitor. That leaves three series, which clear
// the CVD and normal-vision contrast floors on every pair in both themes.
export const COLOR: Record<string, string> = {
  f64: "var(--reference)",
  Dec: "var(--dec)",
  rust_decimal: "var(--rust-decimal)",
  fastnum: "var(--fastnum)",
};

export interface Described {
  operation: string;
  caption: string;
}

/** The order operations are presented in, cheapest kind of work first. */
export const OPERATIONS: Described[] = [
  { operation: "cmp", caption: "Compare two values" },
  { operation: "add", caption: "Add into an accumulator" },
  { operation: "mul", caption: "Multiply price by size" },
  { operation: "div", caption: "Divide price by size" },
  { operation: "sqrt", caption: "Square root of a price" },
  { operation: "sqrt_approx", caption: "Square root through f64" },
  { operation: "round_dp", caption: "Round to 2 decimal places" },
  { operation: "round_to_step", caption: "Round to a 0.01 step" },
  { operation: "floor", caption: "Floor to an integer" },
  { operation: "ceil", caption: "Ceiling to an integer" },
  { operation: "to_f64", caption: "Convert to f64" },
  { operation: "from_f64", caption: "Convert from f64" },
  { operation: "parse", caption: "Parse from a string" },
  { operation: "format", caption: "Format to a string" },
  { operation: "collect", caption: "Parse a batch into a fresh Vec" },
  { operation: "clone", caption: "Allocate and copy a Vec" },
];

export interface SweepDef extends Described {
  heading: string;
  axis: string;
  note: string;
}

/**
 * Groups swept over a parameter rather than measured at one width. These read
 * as a curve, so they get a line chart instead of a row in the table.
 */
export const SWEEPS: SweepDef[] = [
  {
    operation: "parse_digits",
    heading: "Parsing by digit width",
    caption: "Parse from a string",
    axis: "Significant digits in the text",
    note:
      "The parser accumulates in a u64 and promotes to u128 once a mantissa " +
      "passes 19 digits, so the cost of that promotion is visible as the " +
      "curve steepens past the boundary.",
  },
  {
    operation: "format_digits",
    heading: "Formatting by digit width",
    caption: "Format to a string",
    axis: "Significant digits in the value",
    note:
      "Rendering walks the digits it has, so the curve is the cost of the " +
      "digits themselves rather than of any one value.",
  },
];

/**
 * The book groups, reads before writes, with the linear pair last. Every one
 * is swept over depth and measured on `Dec` alone: there is no second
 * implementation to compare against, so these are read as curves against
 * depth rather than as a race.
 */
export const BOOK_OPERATIONS: Described[] = [
  { operation: "book_best", caption: "Best price on one side" },
  { operation: "book_top", caption: "Best bid and ask, mid and spread" },
  { operation: "book_nth", caption: "Reach a level by rank" },
  { operation: "book_find", caption: "Reach a level by price" },
  { operation: "book_update", caption: "Replace the amount at a price held" },
  { operation: "book_apply_diff", caption: "Apply a batch of level updates" },
  { operation: "book_insert", caption: "Add a price the book does not hold" },
  { operation: "book_remove", caption: "Take a level out" },
  { operation: "book_churn", caption: "Insert and evict on a capped book" },
  { operation: "book_stats", caption: "Total size and notional over a side" },
];

export type Table = Map<string, Measurement>;

const key = (operation: string, implementation: string) =>
  `${operation} ${implementation}`;

/** Every measurement taken at a single width, by operation and implementation. */
export function timings(source: Snapshot = snapshot): Table {
  return new Map(
    source.results.map((r) => [key(r.operation, r.implementation), r]),
  );
}

export function at(table: Table, operation: string, implementation: string) {
  return table.get(key(operation, implementation));
}

export type Curve = [parameter: number, nanos: number][];

/** Per implementation, the curve of (parameter, ns) sorted by parameter. */
export function sweepSeries(
  operation: string,
  source: Snapshot = snapshot,
): Map<string, Curve> {
  const series = new Map<string, Curve>();
  for (const record of source.results) {
    if (record.operation !== operation) continue;
    const parameter = Number(record.parameter);
    if (!Number.isFinite(parameter)) continue;
    const curve = series.get(record.implementation) ?? [];
    curve.push([parameter, record.nanos]);
    series.set(record.implementation, curve);
  }
  for (const curve of series.values()) curve.sort((a, b) => a[0] - b[0]);
  return series;
}

export function fastestDecimal(table: Table, operation: string): number | null {
  const candidates = DECIMALS.map(
    (name) => at(table, operation, name)?.nanos,
  ).filter((nanos): nanos is number => nanos !== undefined);
  return candidates.length ? Math.min(...candidates) : null;
}

/** Operations that actually have a measurement, in presentation order. */
export function present(source: Snapshot = snapshot): Described[] {
  const table = timings(source);
  return OPERATIONS.filter((entry) =>
    IMPLEMENTATIONS.some((name) => at(table, entry.operation, name)),
  );
}

export function presentSweeps(source: Snapshot = snapshot): SweepDef[] {
  return SWEEPS.filter((sweep) => sweepSeries(sweep.operation, source).size > 0);
}

export function presentBook(source: Snapshot = snapshot): Described[] {
  return BOOK_OPERATIONS.filter(
    (entry) => sweepSeries(entry.operation, source).size > 0,
  );
}

/** Every depth the book groups were swept over, in order. */
export function bookDepths(source: Snapshot = snapshot): number[] {
  const depths = new Set<number>();
  for (const entry of presentBook(source)) {
    for (const curve of sweepSeries(entry.operation, source).values()) {
      for (const [depth] of curve) depths.add(depth);
    }
  }
  return [...depths].sort((a, b) => a - b);
}

/** Three significant figures is more than the measurement carries. */
export function nanos(value: number): string {
  if (value >= 100) return `${Math.round(value).toLocaleString("en-US")} ns`;
  if (value >= 10) return `${value.toFixed(1)} ns`;
  return `${value.toFixed(2)} ns`;
}
