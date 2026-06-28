# ──────────────────────────────────────────────────────────────────────────────
# Stage 1 — build backend
# ──────────────────────────────────────────────────────────────────────────────
FROM node:24-alpine AS backend-build

WORKDIR /app/backend
COPY backend/package.json backend/package-lock.json* ./
RUN npm ci --ignore-scripts

COPY backend/ ./
RUN npm run build

# ──────────────────────────────────────────────────────────────────────────────
# Stage 2 — build frontend
# ──────────────────────────────────────────────────────────────────────────────
FROM node:24-alpine AS frontend-build

WORKDIR /app/frontend
COPY frontend/package.json frontend/package-lock.json* ./
RUN npm ci --ignore-scripts

COPY frontend/ ./
RUN npm run build

# ──────────────────────────────────────────────────────────────────────────────
# Stage 3 — runtime: Node backend + nginx for static frontend
# ──────────────────────────────────────────────────────────────────────────────
FROM node:24-alpine AS runtime

# Install nginx
RUN apk add --no-cache nginx

# Backend
WORKDIR /app/backend
COPY --from=backend-build /app/backend/dist ./dist
COPY --from=backend-build /app/backend/node_modules ./node_modules
COPY --from=backend-build /app/backend/package.json .

# Frontend static files
COPY --from=frontend-build /app/frontend/dist /usr/share/nginx/html

# Nginx config: serve frontend on :80, proxy /events + /metrics + /health to backend :3100
COPY nginx.conf /etc/nginx/http.d/default.conf

# Start script
COPY start.sh /start.sh
RUN chmod +x /start.sh

EXPOSE 3100 80

CMD ["/start.sh"]
