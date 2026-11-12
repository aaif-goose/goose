FROM oven/bun:1 AS base
WORKDIR /app

# Install dependencies
FROM base AS deps
COPY services/ask-ai-bot/package.json services/ask-ai-bot/bun.lock ./
RUN bun install --frozen-lockfile

# Build stage
FROM base AS build
COPY --from=deps /app/node_modules ./node_modules
COPY services/ask-ai-bot/ ./
RUN bun run build

# Production stage
FROM base AS production
ARG SOURCE_REVISION
ENV NODE_ENV=production
ENV DOCS_PATH=/app/docs
ENV CODEBASE_PATH=/app/codebase
ENV SOURCE_REVISION=$SOURCE_REVISION
ENV THREAD_STATE_PATH=/app/data/threads
VOLUME /app/data

COPY --from=build /app/dist ./dist
COPY --from=build /app/node_modules ./node_modules
COPY --from=build /app/package.json ./

# Copy documentation (only docs/ subdirectory with markdown files)
COPY documentation/docs ./docs

# Copy codebase source for code browsing
COPY ui/ ./codebase/ui/
COPY crates ./codebase/crates
COPY Cargo.toml AGENTS.md ./codebase/

# Empty index.ts for discraft start to detect
RUN touch index.ts

CMD ["bun", "run", "start"]
