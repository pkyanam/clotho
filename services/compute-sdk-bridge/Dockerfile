# ComputeSDK bridge image — pnpm only (no npm).
FROM node:22-alpine
RUN corepack enable && corepack prepare pnpm@11.9.0 --activate
WORKDIR /app
COPY package.json ./
# Install deps; optional @computesdk/* packages may fail individually without
# failing the whole install when using optionalDependencies.
RUN pnpm install --prod --no-frozen-lockfile || pnpm install --prod --no-frozen-lockfile --config.strict-peer-dependencies=false
COPY src ./src
ENV PORT=8091
EXPOSE 8091
CMD ["node", "src/server.mjs"]
