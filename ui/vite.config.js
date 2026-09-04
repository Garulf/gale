import { readFileSync } from 'node:fs';
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

const workspaceVersion = /^version\s*=\s*"([^"]+)"/m.exec(readFileSync(new URL('../Cargo.toml', import.meta.url), 'utf8'))[1];

export default defineConfig({
  base: './',
  define: { __APP_VERSION__: JSON.stringify(workspaceVersion) },
  plugins: [svelte({ compilerOptions: { runes: true } })],
});
