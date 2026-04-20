FROM node:22-alpine
WORKDIR /app
COPY package.json ./
RUN npm install
COPY . .
EXPOSE 8095
CMD ["npx", "vite", "--host", "0.0.0.0", "--port", "8095"]
