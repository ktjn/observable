# syntax=docker/dockerfile:1.28@sha256:bb22d9815c728170f72750f4e5b0d672e06176142e1d602c7e66c050100b7e5b

FROM node:26-alpine@sha256:6bd9ab43c0ec0190ca7b2d193906cb8b7edf6e7f4afa43ddeccd6452710bffb5 AS deps
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
ENV NGINX_ENTRYPOINT_LOCAL_RESOLVERS=1 \
    NGINX_ENVSUBST_FILTER="^(NGINX_LOCAL_RESOLVERS|OBSERVABLE_CATALOG_UPSTREAM)$" \
    OBSERVABLE_CATALOG_UPSTREAM=catalog-service:4325
COPY apps/frontend/nginx.conf /etc/nginx/templates/default.conf.template

EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
