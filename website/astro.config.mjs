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
          href: 'https://github.com/kannandreams/loadout',
        },
      ],
      editLink: {
        baseUrl: 'https://github.com/kannandreams/loadout/blob/main/website/',
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
            { label: 'Introduction', slug: 'index' },
            { label: 'Installation', slug: 'installation' },
            { label: 'Getting Started', slug: 'getting-started' },
            { label: 'CLI Reference', slug: 'cli' },
          ],
        },
        {
          label: 'Capabilities',
          items: [
            { label: 'Overview', slug: 'primitives/overview' },
            { label: 'Primitive Format', slug: 'primitives/format' },
            { label: 'Skills', slug: 'primitives/skills' },
            { label: 'Tools', slug: 'primitives/tools' },
            { label: 'Hooks', slug: 'primitives/hooks' },
            { label: 'Policies', slug: 'primitives/policies' },
            { label: 'Workflows', slug: 'primitives/workflows' },
          ],
        },
        {
          label: 'How Coral Works',
          items: [
            { label: 'Development Lifecycle', slug: 'concepts/development-lifecycle' },
            { label: 'Lifecycle & Drift Detection', slug: 'concepts/lifecycle' },
            { label: 'Diffing & Updates', slug: 'concepts/diff-update' },
            { label: 'Scopes & Overrides', slug: 'concepts/scopes' },
            { label: 'Harness Adapters', slug: 'concepts/adapters' },
            { label: 'Lockfile Reference', slug: 'concepts/lockfile' },
          ],
        },
        {
          label: 'Product Model',
          items: [
            { label: 'Repository Model', slug: 'repository-model' },
            { label: 'Usage Scenarios', slug: 'usage-scenarios' },
          ],
        },
        {
          label: 'Project',
          items: [
            { label: 'Roadmap', slug: 'roadmap' },
            { label: 'Development', slug: 'development' },
            { label: 'Comparison: Vercel Skills', slug: 'comparison/vercel-skills' },
          ],
        },
      ],
    }),
  ],
});
