# Playground Dockerfile
# Extends the base howth image with Node.js and playground files

FROM ghcr.io/jschatz1/howth:latest

# Install Node.js
RUN apt-get update && \
    apt-get install -y --no-install-recommends nodejs && \
    rm -rf /var/lib/apt/lists/*

# Copy playground files
COPY examples/playground /app/playground

WORKDIR /app/playground

ENV HOWTH_BIN=/usr/local/bin/howth
ENV PORT=3001

EXPOSE 3001

CMD ["node", "server.js"]
