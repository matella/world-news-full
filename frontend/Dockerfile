# Multi-stage build for the World News frontend (Next.js App Router, standalone output).
# Deps + build happen once at image-build time; the runtime image just runs the traced server —
# starts in milliseconds, needs no network, fully reproducible (reboot-proof).
# syntax=docker/dockerfile:1

FROM node:24-alpine AS deps
WORKDIR /app
COPY package.json package-lock.json* ./
# npm install (not ci): package.json pins several deps to "latest", which a fixed lockfile can't
# satisfy under ci's strict equality. install resolves them and still bakes deps into the image.
RUN npm install --no-audit --no-fund

FROM node:24-alpine AS build
WORKDIR /app
COPY --from=deps /app/node_modules ./node_modules
COPY . .
RUN npm run build

FROM node:24-alpine AS runner
WORKDIR /app
ENV NODE_ENV=production
ENV PORT=3000
ENV HOSTNAME=0.0.0.0
# standalone output bundles a minimal node_modules + server.js; static assets are copied alongside.
COPY --from=build /app/.next/standalone ./
COPY --from=build /app/.next/static ./.next/static
EXPOSE 3000
CMD ["node", "server.js"]
