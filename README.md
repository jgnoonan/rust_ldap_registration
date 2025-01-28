# Signal Registration Service

A Rust-based gRPC service for user registration with LDAP authentication, Twilio phone verification, and DynamoDB storage.

## Prerequisites

- Rust (latest stable version)
- An LDAP server (e.g., OpenLDAP)
- AWS DynamoDB (local or cloud)
- Twilio account (for phone verification)
- Protocol Buffers compiler

### LDAP Server Requirements

The LDAP server must:
- Support simple bind authentication
- Have users with the following attributes:
  - `uid` or configurable username attribute
  - `mobile` or configurable phone number attribute
- Be accessible from the service host

## Configuration

1. Copy the example configuration:
```bash
cp config/application.yml.example config/application.yml
```

2. Update the configuration with your environment-specific values:
```yaml
registration:
  ldap:
    url: "ldap://your-ldap-server:389"
    base_dn: "dc=example,dc=com"
    bind_dn: "cn=admin,dc=example,dc=com"
    bind_password: "your-bind-password"
```

### Environment Variables

For production deployment, use environment variables for sensitive data:
- `LDAP_BIND_PASSWORD`: LDAP bind password
- `TWILIO_ACCOUNT_SID`: Twilio account SID
- `TWILIO_AUTH_TOKEN`: Twilio auth token
- `TWILIO_VERIFY_SERVICE_SID`: Twilio Verify service SID

## Building

1. Install build dependencies:
```bash
# Ubuntu/Debian
sudo apt-get install -y protobuf-compiler libssl-dev pkg-config

# macOS
brew install protobuf
```

2. Build the project:
```bash
cargo build --release
```

## Running

1. Start the service:
```bash
cargo run --release
```

The service will start on port 50051 by default.

2. For development with a local DynamoDB:
```bash
# Start local DynamoDB
docker run -p 8000:8000 amazon/dynamodb-local

# Create required table
aws dynamodb create-table \
    --table-name signal_accounts \
    --attribute-definitions AttributeName=phone_number,AttributeType=S \
    --key-schema AttributeName=phone_number,KeyType=HASH \
    --provisioned-throughput ReadCapacityUnits=5,WriteCapacityUnits=5 \
    --endpoint-url http://localhost:8000
```

## Testing

Run the test suite:
```bash
cargo test
```

### Manual Testing

You can use the provided test client:
```bash
cargo run --bin test-client -- --username test.user --password userpass
```

## Monitoring

The service exposes metrics on port 9090 and can be integrated with:
- Prometheus
- Datadog (when enabled in configuration)

## License

Copyright 2025 Joseph G Noonan

Licensed under the AGPLv3 license.
