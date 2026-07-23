import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://coral.dev',
  integrations: [
    starlight({
      title: 'Coral',
      description: 'Capability lifecycle management for coding agents.',
      customCss: ['./src/styles/custom.css'],
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: 'https://github.com/kannandreams/coral',
        },
      ],
      editLink: {
        baseUrl: 'https://github.com/kannandreams/coral/blob/main/website/',
      },
      lastUpdated: true,
      expressiveCode: {
        themes: ['github-dark', 'github-light'],
      },
      tableOfContents: {
        minHeadingLevel: 2,
        maxHeadingLevel: 3,
      },
      sidebar: [
        {
          label: 'Start Here',
          items: [
            { label: 'Intro', slug: 'index' },
            { label: 'Installation', slug: 'installation' },
            { label: 'Getting Started', slug: 'getting-started' },
            { label: 'CLI Reference', slug: 'cli' },
            { label: 'Harness Config', slug: 'harness-cli-cheatsheet' },
          ],
        },
        {
          label: 'Capabilities',
          items: [
            { label: 'Overview', slug: 'primitives/overview' },
            { label: 'coral.toml', slug: 'primitives/format' },
            { label: 'Skills', slug: 'primitives/skills' },
            { label: 'Tools', slug: 'primitives/tools' },
            { label: 'Hooks', slug: 'primitives/hooks' },
            { label: 'Policies', slug: 'primitives/policies' },
            { label: 'Workflows', slug: 'primitives/workflows' },
          ],
        },
        {
          label: 'Workflows & Operations',
          items: [
            { label: 'When to Use Coral', slug: 'usage-scenarios' },
            { label: 'Use Cases Overview', slug: 'concepts/development-lifecycle' },
            { label: 'Lifecycle & Drift Detection', slug: 'concepts/lifecycle' },
            { label: 'Diffing & Updates', slug: 'concepts/diff-update' },
            { label: 'Scopes & Overrides', slug: 'concepts/scopes' },
            { label: 'Harness Adapters', slug: 'concepts/adapters' },
            { label: 'Lockfile Reference', slug: 'concepts/lockfile' },
          ],
        },
        {
          label: 'Develop',
          items: [
            { label: 'Development', slug: 'development' },
            { label: 'Credits', slug: 'credits' },
          ],
        },
      ],
    }),
  ],
});
