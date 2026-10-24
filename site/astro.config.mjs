// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// TODO: add `site: 'https://<your-domain>'` once the Vercel domain is known.
export default defineConfig({
  integrations: [
    starlight({
      title: 'My IDM',
      sidebar: [
        {
          label: 'Documentation',
          items: [
            { label: 'Getting started', slug: 'getting-started' },
            { label: 'Installation', slug: 'installation' },
            { label: 'Using the app', slug: 'usage' },
            { label: 'Troubleshooting', slug: 'troubleshooting' },
            { label: 'FAQ', slug: 'faq' },
          ],
        },
      ],
    }),
  ],
});
