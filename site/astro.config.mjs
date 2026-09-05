// @ts-check
import react from "@astrojs/react";
import tailwind from "@tailwindcss/vite";
import { defineConfig } from "astro/config";

import { rustdoc } from "./src/lib/rustdoc.mjs";

// Served from the root of its own domain. The custom domain is configured in
// the repository's Pages settings rather than a committed CNAME, which is what
// a workflow-deployed site does: there is no file here that records it, so
// `site` is the only place the canonical host is written down.
export default defineConfig({
  site: "https://troy.quantmind.com",
  trailingSlash: "ignore",
  integrations: [react()],
  markdown: { remarkPlugins: [rustdoc("/")] },
  vite: {
    plugins: [tailwind()],
    // docs/bench-data.json and docs/*.md live above the Astro root
    server: { fs: { allow: [".."] } },
  },
});
