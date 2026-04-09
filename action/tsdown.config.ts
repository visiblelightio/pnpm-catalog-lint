import { defineConfig } from "tsdown";

export default defineConfig({
  entry: ["./src/index.ts"],
  clean: false,
  outDir: "./",
  format: "esm",
  platform: "node",
  outputOptions: {
    entryFileNames: "[name].js",
  },
  deps: {
    alwaysBundle: [/.*/],
    onlyBundle: false,
  },
});
