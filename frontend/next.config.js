/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  // Build a self-contained server bundle (minimal traced node_modules) → tiny runtime image that
  // starts in milliseconds with no install/build step. Runner copies .next/standalone + static.
  output: "standalone",
}
module.exports = nextConfig
