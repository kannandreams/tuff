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
      components: {
        SocialIcons: './src/components/SocialIcons.astro',
      },
      // Starlight emits a large-image Twitter card but no image, so a shared
      // docs link would render without one. Reuse the homepage banner.
      head: [
        { tag: 'meta', attrs: { property: 'og:image', content: 'https://tuffcli.dev/img/tuff-readme-banner.png' } },
        { tag: 'meta', attrs: { property: 'og:image:width', content: '1950' } },
        { tag: 'meta', attrs: { property: 'og:image:height', content: '807' } },
        { tag: 'meta', attrs: { property: 'og:image:alt', content: 'Tuff: capability lifecycle manager for coding agents' } },
        { tag: 'meta', attrs: { name: 'twitter:image', content: 'https://tuffcli.dev/img/tuff-readme-banner.png' } },
      ],
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
        {
          icon: 'vscode',
          label: 'VS Code Marketplace',
          href: 'https://marketplace.visualstudio.com/items?itemName=kannandreams.tuff',
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
            { label: 'What Is a Capability', slug: 'primitives/overview' },
            { label: 'The tuff.toml File', slug: 'primitives/format' },
            { label: 'The tuff.lock File', slug: 'concepts/lockfile' },
          ],
        },
        {
          label: 'Capabilities',
          items: [
            { label: 'Skills', slug: 'primitives/skills' },
            { label: 'Tools', slug: 'primitives/tools' },
            {
              label: 'MCP Servers',
              items: [
                { label: 'Overview', slug: 'primitives/mcp-servers' },
                { label: 'MCP Catalog', link: '/mcp-catalog' },
              ],
            },
            {
              label: 'Hooks',
              items: [
                { label: 'Overview', slug: 'primitives/hooks' },
                { label: 'Hooks Specification', slug: 'spec/hooks' },
              ],
            },
            { label: 'Policies', slug: 'primitives/policies' },
            { label: 'Workflows', slug: 'primitives/workflows' },
          ],
        },
        {
          label: 'CLI Reference',
          collapsed: true,
          items: [
            { label: 'Overview', slug: 'cli' },
            { label: 'Create and Add', slug: 'cli/add' },
            { label: 'Packs', slug: 'cli/packs' },
            { label: 'MCP Servers', slug: 'cli/mcp' },
            { label: 'Inspect and Generate', slug: 'cli/inspect' },
            { label: 'Diff and Update', slug: 'cli/diff-update' },
            { label: 'Validate in CI', slug: 'cli/ci' },
            { label: 'Clean Up', slug: 'cli/clean-up' },
            { label: 'Agents and Scope', slug: 'cli/agents' },
          ],
        },
        {
          label: 'Packaging & Distribution',
          items: [
            { label: 'Capability Packs', slug: 'concepts/packs' },
            { label: 'OCI Registries & Container Images', slug: 'guides/oci-registries-and-container-images' },
          ],
        },
        {
          label: 'Integrations',
          items: [
            { label: 'Claude Code Plugin', slug: 'guides/claude-code-plugin' },
            { label: 'VS Code Extension', slug: 'guides/vscode-extension' },
            { label: 'Harness Adapters', slug: 'concepts/adapters' },
            { label: 'Harness Config', slug: 'harness-cli-cheatsheet' },
          ],
        },
        {
          label: 'Concepts',
          items: [
            { label: 'When to Use Tuff', slug: 'usage-scenarios' },
            { label: 'Use Cases Overview', slug: 'concepts/development-lifecycle' },
            { label: 'Lifecycle & Drift Detection', slug: 'concepts/lifecycle' },
            { label: 'Diffing & Updates', slug: 'concepts/diff-update' },
            { label: 'Scopes & Overrides', slug: 'concepts/scopes' },
          ],
        },
        {
          label: 'Project',
          items: [
            { label: 'Changelog', slug: 'changelog' },
            { label: 'Development', slug: 'development' },
            { label: 'Credits', slug: 'credits' },
            { label: 'Privacy', slug: 'privacy' },
          ],
        },
      ],
    }),
  ],
});
