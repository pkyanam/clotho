FROM node:22-alpine AS build

WORKDIR /workspace
RUN npm install --global pnpm@11.9.0

COPY package.json pnpm-lock.yaml pnpm-workspace.yaml turbo.json ./
COPY apps/web/package.json apps/web/package.json
COPY packages/config/package.json packages/config/package.json
COPY packages/sdk-js/package.json packages/sdk-js/package.json
COPY packages/ui/package.json packages/ui/package.json
RUN pnpm install --frozen-lockfile

COPY apps/web apps/web
COPY packages/config packages/config
COPY packages/sdk-js packages/sdk-js
COPY packages/ui packages/ui
RUN pnpm --filter @clotho/web build

FROM node:22-alpine AS runtime

ENV NODE_ENV=production \
    HOSTNAME=0.0.0.0 \
    PORT=3100
WORKDIR /app

COPY --from=build /workspace/apps/web/.next/standalone ./
COPY --from=build /workspace/apps/web/.next/static ./apps/web/.next/static

USER node
EXPOSE 3100
CMD ["node", "apps/web/server.js"]

