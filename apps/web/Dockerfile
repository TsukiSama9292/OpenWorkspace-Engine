# ---- Builder ---------------------------------------------------------------
# Build the SvelteKit static site inside the pnpm workspace. The build context
# is the repository root (see docker-compose.yml) so the lockfile and workspace
# manifests are available.
FROM node:22-alpine AS builder

RUN corepack enable

WORKDIR /app

# Install against the lockfile first so dependency layers stay cached across
# source changes. Every workspace manifest the lockfile references must be
# present.
COPY pnpm-workspace.yaml pnpm-lock.yaml package.json ./
COPY apps/web/package.json ./apps/web/package.json
COPY apps/api/package.json ./apps/api/package.json
RUN pnpm install --frozen-lockfile

# Real sources.
COPY apps/web ./apps/web
RUN pnpm --filter web build

# ---- Runtime ---------------------------------------------------------------
FROM nginx:alpine

COPY --from=builder /app/apps/web/build /usr/share/nginx/html

# Static site + SPA fallback. The app routes /kasmvnc/<token>/ and /api/ via
# Traefik (same origin); nginx only ever sees requests for static assets, but
# the catch-all fallback keeps unknown paths served by the SvelteKit shell.
COPY <<'EOF' /etc/nginx/conf.d/default.conf
server {
    listen 80;
    server_name _;
    root /usr/share/nginx/html;
    index index.html;

    gzip on;
    gzip_types text/css application/javascript application/json image/svg+xml;

    location /_app/ {
        expires 30d;
        add_header Cache-Control "public, immutable";
        try_files $uri =404;
    }

    location / {
        try_files $uri $uri/ /index.html;
    }
}
EOF

EXPOSE 80

HEALTHCHECK --interval=15s --timeout=5s --start-period=5s --retries=5 \
    CMD wget -qO- http://127.0.0.1/ >/dev/null || exit 1
