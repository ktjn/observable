#!/bin/sh
set -e

# Start the Node.js backend in the background
node /app/backend/dist/index.js &

# Start nginx in the foreground
nginx -g "daemon off;"
