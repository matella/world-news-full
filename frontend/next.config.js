/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  // Build a self-contained server bundle (minimal traced node_modules) → tiny runtime image that
  // starts in milliseconds with no install/build step. Runner copies .next/standalone + static.
  output: "standalone",
  // Homelab site: don't let a stray type/lint nit fail the production build on reboot. The build
  // still compiles via SWC; we just skip the strict type-check + lint gate that `next build` runs.
  typescript: { ignoreBuildErrors: true },
  eslint: { ignoreDuringBuilds: true },
}
module.exports = nextConfig
