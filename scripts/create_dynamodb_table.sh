#!/bin/bash

# Default values
TABLE_NAME="signal_accounts"
REGION="us-west-2"
ENDPOINT="http://localhost:8000"

# Parse command line arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --table-name)
      TABLE_NAME="$2"
      shift 2
      ;;
    --region)
      REGION="$2"
      shift 2
      ;;
    --endpoint)
      ENDPOINT="$2"
      shift 2
      ;;
    *)
      echo "Unknown option: $1"
      exit 1
      ;;
  esac
done

# Create the table
aws dynamodb create-table \
  --table-name "$TABLE_NAME" \
  --attribute-definitions \
    AttributeName=phone_number,AttributeType=S \
  --key-schema \
    AttributeName=phone_number,KeyType=HASH \
  --billing-mode PAY_PER_REQUEST \
  --endpoint-url "$ENDPOINT" \
  --region "$REGION"

# Wait for the table to be active
aws dynamodb wait table-exists \
  --table-name "$TABLE_NAME" \
  --endpoint-url "$ENDPOINT" \
  --region "$REGION"

# Add tags
aws dynamodb tag-resource \
  --resource-arn "arn:aws:dynamodb:$REGION:000000000000:table/$TABLE_NAME" \
  --tags Key=Service,Value=Registration \
  --endpoint-url "$ENDPOINT" \
  --region "$REGION"

echo "Table $TABLE_NAME created successfully"
