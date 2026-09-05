import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import starlightBlog from 'starlight-blog';

export default defineConfig({
  site: 'https://tuffcli.dev',
  devToolbar: {
    enabled: false,
  },
  integrations: [
    starlight({
      title: 'Tuff',
      description: 'Capability lifecycle management for coding agents.',
      favicon: '/favicon.svg',
      customCss: ['./src/styles/custom.css'],
      plugins: [
        starlightBlog({
          title: 'Blog',
          navigation: 'header-end',
          postCount: 10,
          authors: {
            kannan: {
              name: 'Kannan Kalidasan',
              title: 'Tuff maintainer',
              url: 'https://github.com/kannandreams',
            },
          },
        }),
      ],
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: 'https://github.com/kannandreams/tuff',
        },
      ],
      editLink: {
        baseUrl: 'https://github.com/kannandreams/tuff/blob/main/website/',
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
            { label: 'Intro', slug: 'intro' },
            { label: 'Installation', slug: 'installation' },
            { label: 'Getting Started', slug: 'getting-started' },
            { label: 'CLI Reference', slug: 'cli' },
            { label: 'MCP Catalog', slug: 'mcp-catalog' },
            { label: 'Harness Config', slug: 'harness-cli-cheatsheet' },
            { label: 'Changelog', slug: 'changelog' },
          ],
        },
        {
          label: 'Capabilities',
          items: [
            { label: 'Overview', slug: 'primitives/overview' },
            { label: 'tuff.toml', slug: 'primitives/format' },
            { label: 'Skills', slug: 'primitives/skills' },
            { label: 'Tools', slug: 'primitives/tools' },
            { label: 'MCP Servers', slug: 'primitives/mcp-servers' },
            { label: 'Hooks', slug: 'primitives/hooks' },
            { label: 'Policies', slug: 'primitives/policies' },
            { label: 'Workflows', slug: 'primitives/workflows' },
            { label: 'Capability Packs', slug: 'concepts/packs' },
          ],
        },
        {
          label: 'Workflows & Operations',
          items: [
            { label: 'When to Use Tuff', slug: 'usage-scenarios' },
            { label: 'OCI Registries & Container Images', slug: 'guides/oci-registries-and-container-images' },
            { label: 'Claude Code Plugin', slug: 'guides/claude-code-plugin' },
            { label: 'VS Code Extension', slug: 'guides/vscode-extension' },
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
            { label: 'Privacy', slug: 'privacy' },
          ],
        },
      ],
    }),
  ],
});
