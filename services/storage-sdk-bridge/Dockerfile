FROM node:22-alpine
WORKDIR /app
RUN npm install --global pnpm@11.9.0
COPY package.json ./
RUN pnpm install --prod
COPY src ./src
ENV PORT=8092
EXPOSE 8092
USER node
CMD ["node", "src/server.mjs"]

