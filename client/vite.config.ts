import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import sveltePreprocess from 'svelte-preprocess';
import Checker from 'vite-plugin-checker'
import * as path from 'path';
import * as fs from 'node:fs';

const packageJson = JSON.parse(fs.readFileSync('./package.json', 'utf-8'));

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [
    svelte({preprocess: [sveltePreprocess({ typescript: true })]}),
    Checker({ typescript: true }),
  ],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
      '@clapshot_protobuf': path.resolve(__dirname, '../protobuf/libs')
    },
  },
  define: {
    'process.env.CLAPSHOT_MIN_SERVER_VERSION': JSON.stringify(packageJson.clapshot_min_server_version),
    'process.env.CLAPSHOT_MAX_SERVER_VERSION': JSON.stringify(packageJson.clapshot_max_server_version),
    'process.env.CLAPSHOT_CLIENT_VERSION': JSON.stringify(packageJson.version),
  },
});
