// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import tailwindcss from '@tailwindcss/vite';

// The feed and the sitemap need an absolute origin. Vercel supplies one on every
// deploy; anywhere else, set SITE_URL.
const site =
  process.env.SITE_URL ||
  (process.env.VERCEL_PROJECT_PRODUCTION_URL
    ? `https://${process.env.VERCEL_PROJECT_PRODUCTION_URL}`
    : 'http://localhost:4321');

export default defineConfig({
  site,
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
      head: [
        {
          tag: 'link',
          attrs: {
            rel: 'alternate',
            type: 'application/rss+xml',
            title: 'B Download Manager documentation',
            href: '/rss.xml',
          },
        },
      ],
      components: {
        Header: './src/components/docs/Header.astro',
        MobileMenuFooter: './src/components/docs/MobileMenuFooter.astro',
        ThemeSelect: './src/components/docs/ThemeToggle.astro',
        TableOfContents: './src/components/docs/TableOfContents.astro',
      },
      sidebar: [
        {
          label: 'Documentation',
          items: [
            { label: 'Getting started', slug: 'getting-started' },
            { label: 'Getting the app', slug: 'installation' },
            { label: 'Using the app', slug: 'usage' },
            { label: 'Toolbar packs', slug: 'toolbars' },
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
