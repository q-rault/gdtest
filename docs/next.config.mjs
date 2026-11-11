import { createMDX } from 'fumadocs-mdx/next';

const withMDX = createMDX();

// The site is served from https://q-rault.github.io/gdtest/, so every URL
// lives under /gdtest. Set DOCS_BASE_PATH='' to serve from a domain root.
const basePath = process.env.DOCS_BASE_PATH ?? '/gdtest';

/** @type {import('next').NextConfig} */
const config = {
  reactStrictMode: true,
  output: 'export',
  basePath,
  images: { unoptimized: true },
  env: { NEXT_PUBLIC_BASE_PATH: basePath },
};

export default withMDX(config);
