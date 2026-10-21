# syntax=docker/dockerfile:1.28@sha256:bb22d9815c728170f72750f4e5b0d672e06176142e1d602c7e66c050100b7e5b

FROM node:24-alpine@sha256:6a1c190e17215d844926b0677f4bf87273ff7d5e54db06d3b5b5863ab0b09a3d AS deps
WORKDIR /app

COPY package.json package-lock.json ./
COPY apps/frontend/package.json apps/frontend/package.json
RUN --mount=type=cache,id=observable-npm,target=/root/.npm,sharing=locked \
    npm ci --workspace=apps/frontend --ignore-scripts

FROM deps AS build
COPY apps/frontend apps/frontend
WORKDIR /app/apps/frontend
ARG VITE_OBSERVABLE_SELF_OBSERVABILITY_MODE=self
ARG VITE_OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT=
ENV VITE_OBSERVABLE_SELF_OBSERVABILITY_MODE=$VITE_OBSERVABLE_SELF_OBSERVABILITY_MODE
ENV VITE_OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT=$VITE_OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT
RUN npm run build

FROM nginx:1.31-alpine@sha256:892bb2152464f0c5d8972593a5f5afaf8054552580b699d95f8ff4af08c2d33d AS runtime

RUN rm -rf /usr/share/nginx/html/*
COPY --from=build /app/apps/frontend/dist /usr/share/nginx/html
COPY apps/frontend/nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
