import { visit } from "unist-util-visit";

// docs/*.md is included into the crate docs with `#[doc = include_str!(..)]`,
// so it is written for rustdoc and carries rustdoc's intra-doc links. The same
// text is published here, which means the two rustdoc-isms have to be adapted
// rather than left to render as a broken relative link.
const DOCS_RS = "https://docs.rs/troy/latest/troy";

// `crate::memory_layout` is a doc module that exists on this site as a page of
// its own, so it stays inside the site. Everything else is API and belongs on
// docs.rs, where an all-caps tail is an associated constant and a lower-case
// one a method.
function target(path, base) {
  if (path === "memory_layout") return `${base}docs/memory-layout`;

  const [item, member] = path.split("::");
  if (!item || !/^[A-Z]/.test(item)) return null;
  const page = `${DOCS_RS}/struct.${item}.html`;
  if (!member) return page;
  const kind = /^[A-Z0-9_]+$/.test(member) ? "associatedconstant" : "method";
  return `${page}#${kind}.${member}`;
}

/**
 * Strip the leading `# Heading` so the page header is rendered once, by the
 * layout, for every document alike, and rewrite `crate::` links. A link with
 * no derivable target is unwrapped to its own text: a missing link reads
 * better than one that 404s.
 */
export function rustdoc(base) {
  return () => (tree) => {
    const first = tree.children[0];
    if (first && first.type === "heading" && first.depth === 1) {
      tree.children.shift();
    }

    visit(tree, "link", (node, index, parent) => {
      if (!node.url.startsWith("crate::")) return;
      const url = target(node.url.slice("crate::".length), base);
      if (url) {
        node.url = url;
      } else if (parent && index !== null) {
        parent.children.splice(index, 1, ...node.children);
      }
    });
  };
}
