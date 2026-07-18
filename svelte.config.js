import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

export default {
  // Svelte 5; consult docs/design.md §7 for component conventions.
  preprocess: vitePreprocess(),
};
