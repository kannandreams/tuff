import type {SidebarsConfig} from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  docs: [
    'index',
    'usage-scenarios',
    'installation',
    'getting-started',
    'repository-model',
    'roadmap',
    'cli',
    'primitive-format',
    {
      type: 'category',
      label: 'Comparison',
      items: ['comparison/vercel-skills'],
    },
    'development',
  ],
};

export default sidebars;
