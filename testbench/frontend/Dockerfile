FROM node:20-slim

LABEL org.opencontainers.image.version="0.1.0"

WORKDIR /app
COPY package.json .
RUN npm install --omit=dev

COPY tracing.js server.js ./

ENV NODE_ENV=production
CMD ["npm", "start"]
