import type {Config} from '@docusaurus/types';
import type {Options as PresetOptions} from '@docusaurus/preset-classic';
import {themes} from 'prism-react-renderer';

const config: Config = {
  title: 'Loadout',
  tagline: 'Lifecycle management for project-owned agent capabilities.',
  favicon: 'img/favicon.ico',

  url: 'https://loadout.dev',
  baseUrl: '/',

  organizationName: 'loadout',
  projectName: 'loadout',

  onBrokenLinks: 'throw',
  markdown: {
    hooks: {
      onBrokenMarkdownLinks: 'warn',
    },
  },

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      {
        docs: {
          sidebarPath: './sidebars.ts',
          routeBasePath: '/',
          editUrl: 'https://github.com/kannandreams/loadout/tree/main/',
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies PresetOptions,
    ],
  ],

  themeConfig: {
    navbar: {
      title: 'Loadout',
      items: [
        {
          type: 'docSidebar',
          sidebarId: 'docs',
          position: 'left',
          label: 'Docs',
        },
        {
          label: 'Roadmap',
          to: '/roadmap',
          position: 'left',
        },
        {
          href: 'https://github.com/kannandreams/loadout',
          label: 'GitHub',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Docs',
          items: [
            {
              label: 'Getting Started',
              to: '/getting-started',
            },
            {
              label: 'Usage Scenarios',
              to: '/usage-scenarios',
            },
            {
              label: 'Roadmap',
              to: '/roadmap',
            },
            {
              label: 'Skills.sh Comparison',
              to: '/comparison/vercel-skills',
            },
          ],
        },
      ],
      copyright: `Copyright © ${new Date().getFullYear()} Loadout contributors.`,
    },
    prism: {
      theme: themes.github,
      darkTheme: themes.dracula,
    },
  },
};

export default config;
