#!/bin/bash

# Default values
PORT=8000
CONTAINER_NAME="registration-dynamodb-local"

# Parse command line arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --port)
      PORT="$2"
      shift 2
      ;;
    --container-name)
      CONTAINER_NAME="$2"
      shift 2
      ;;
    *)
      echo "Unknown option: $1"
      exit 1
      ;;
  esac
done

# Stop and remove existing container if it exists
docker stop "$CONTAINER_NAME" 2>/dev/null
docker rm "$CONTAINER_NAME" 2>/dev/null

# Start DynamoDB Local
docker run -d \
  --name "$CONTAINER_NAME" \
  -p "$PORT":8000 \
  amazon/dynamodb-local:latest \
  -jar DynamoDBLocal.jar -sharedDb

echo "DynamoDB Local started on port $PORT"
