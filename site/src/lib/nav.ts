/** Astro normalises `base` inconsistently across versions, so trim it once. */
const BASE = import.meta.env.BASE_URL.replace(/\/+$/, "");

/** An in-site path, prefixed with the base the site is published under. */
export const href = (path: string) => (path ? `${BASE}/${path}` : `${BASE}/`);

export interface NavItem {
  path: string;
  label: string;
  /**
   * The longer name, for where there is room to spell it out: the front page
   * pills. The nav itself uses `label`, which has a row to fit into.
   */
  full?: string;
}

/**
 * Every page below the front one, in reading order. Adding a page means adding
 * a line here: it appears in the top nav and as a pill on the front page, so
 * the two cannot drift apart.
 */
export const NAV: NavItem[] = [
  { path: "dec", label: "Dec", full: "Dec benchmarks" },
  { path: "orderbook", label: "Order book", full: "Order book benchmarks" },
  { path: "docs/design", label: "Design" },
  { path: "docs/memory-layout", label: "Memory layout" },
  { path: "docs/release-notes", label: "Release notes" },
];

export interface Link {
  href: string;
  label: string;
}

/**
 * Where the crate lives. Rendered at both ends of the page, from here, so the
 * nav and the footer cannot drift apart.
 */
export const LINKS: Link[] = [
  { href: "https://github.com/quantmind/troy", label: "GitHub" },
  { href: "https://docs.rs/troy", label: "docs.rs" },
  { href: "https://crates.io/crates/troy", label: "crates.io" },
];

export interface DocMeta {
  title: string;
  lede: string;
}

/**
 * Headings for the pages rendered from docs/*.md. They live here rather than
 * in frontmatter because the same files are included into the crate docs with
 * `include_str!`, where frontmatter would render as body text.
 */
export const DOCS: Record<string, DocMeta> = {
  design: {
    title: "Design",
    lede: "The decisions behind Dec and the reasoning that led to them: how a value is represented, how an operation reports failure, and how two values compare.",
  },
  "memory-layout": {
    title: "Memory layout",
    lede: "What a Dec is in memory, what that costs, and why the scale is a compile-time constant rather than a field.",
  },
  "release-notes": {
    title: "Release notes",
    lede: "One section per tagged release, newest first.",
  },
};
