# syntax=docker/dockerfile:1.29@sha256:572cebbc2b15e6f84c077742952674889cb63cd5e9b80abdffcbe587b6f19183

FROM node:24-alpine@sha256:116b800c3754b440ee0aa3ed2f20f5bf3d99a554bfd9adeea09ea8017f6e85f5 AS deps
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

FROM nginx:1.31-alpine@sha256:fed7e14be83cbd2661a75cdc5b872d936cf155d46c327084d30b8df19ae88d72 AS runtime

RUN rm -rf /usr/share/nginx/html/*
COPY --from=build /app/apps/frontend/dist /usr/share/nginx/html
ENV NGINX_ENTRYPOINT_LOCAL_RESOLVERS=1 \
    NGINX_ENVSUBST_FILTER="^(NGINX_LOCAL_RESOLVERS|OBSERVABLE_CATALOG_UPSTREAM)$" \
    OBSERVABLE_CATALOG_UPSTREAM=catalog-service:4325
COPY apps/frontend/nginx.conf /etc/nginx/templates/default.conf.template

EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
