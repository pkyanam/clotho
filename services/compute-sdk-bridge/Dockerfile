# ComputeSDK bridge image — pnpm only (no npm).
FROM node:22-alpine
RUN corepack enable && corepack prepare pnpm@11.9.0 --activate
WORKDIR /app
# pnpm 11 reads allowBuilds from pnpm-workspace.yaml (not package.json).
COPY package.json pnpm-workspace.yaml ./
RUN pnpm install --prod --no-frozen-lockfile
COPY src ./src
ENV PORT=8091
EXPOSE 8091
USER node
CMD ["node", "src/server.mjs"]
