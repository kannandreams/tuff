import type {SidebarsConfig} from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  docs: [
    'index',
    'installation',
    'getting-started',
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
