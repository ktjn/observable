FROM node:24-alpine
WORKDIR /app
COPY package.json ./
RUN npm install --legacy-peer-deps
COPY . .
EXPOSE 8095
CMD ["npx", "vite", "--host", "0.0.0.0", "--port", "8095"]
