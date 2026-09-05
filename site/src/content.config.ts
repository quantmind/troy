import { glob } from "astro/loaders";
import { defineCollection } from "astro:content";

/**
 * The prose pages are the crate's own docs, published unchanged. They are
 * loaded from ../docs rather than copied into the site so there is one copy of
 * each: the same file backs the page here and the chapter in `cargo doc`.
 */
export const collections = {
  docs: defineCollection({
    loader: glob({
      pattern: ["design.md", "memory-layout.md", "release-notes.md"],
      base: "../docs",
    }),
  }),
};
