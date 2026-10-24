// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import tailwindcss from '@tailwindcss/vite';

// TODO: add `site: 'https://<your-domain>'` once the Vercel domain is known.
export default defineConfig({
  vite: {
    plugins: [tailwindcss()],
  },
  integrations: [
    starlight({
      title: 'B Download Manager',
      favicon: '/images/app-icon.png',
      logo: {
        src: './public/images/app-icon.png',
      },
      customCss: ['./src/styles/starlight.css'],
      sidebar: [
        {
          label: 'Documentation',
          items: [
            { label: 'Getting started', slug: 'getting-started' },
            { label: 'Getting the app', slug: 'installation' },
            { label: 'Using the app', slug: 'usage' },
            { label: 'Troubleshooting', slug: 'troubleshooting' },
            { label: 'FAQ', slug: 'faq' },
          ],
        },
        {
          label: 'For developers',
          items: [{ label: 'Building from source', slug: 'developers' }],
        },
      ],
    }),
  ],
});
