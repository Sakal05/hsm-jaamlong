#!/usr/bin/env bash
set -x
set -eo pipefail

DB_PORT="${REDIS_PORT:=6379}"
DB_NAME="${REDIS_NAME:=redis_$(date '+%s')}"
DB_HOST="${POSTGRES_HOST:=localhost}"

# Allow to skip Redis if a dockerized Redis database is already running
if [[ -z "${SKIP_REDIS}" ]]; then
  # if a redis container is running, print instructions to kill it and exit
  RUNNING_REDIS_CONTAINER=$(docker ps --filter 'name=redis' --format '{{.ID}}')
  if [[ -n $RUNNING_REDIS_CONTAINER ]]; then
    echo >&2 "There is a Redis container already running, kill it with"
    echo >&2 "    docker kill ${RUNNING_REDIS_CONTAINER}"
    exit 1
  fi
  # Launch Redis using Docker
  docker run \
      --name "${DB_NAME}" \
      -p "${DB_PORT}":6379 \
      -d redis
      # ^ Increased maximum number of connections for testing purposes
fi

# Keep pinging Redis until it's ready to accept commands
until docker exec -i "${DB_NAME}" redis-cli -u "redis://${DB_HOST}:${DB_PORT}" PING; do
  >&2 echo "Redis is still unavailable - sleeping"
  sleep 1
done

>&2 echo "Redis is up and running on port ${DB_PORT}! You are ready to go!"
