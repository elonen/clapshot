import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import sveltePreprocess from 'svelte-preprocess';
import Checker from 'vite-plugin-checker'
import * as path from 'path';

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
});
