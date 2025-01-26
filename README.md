# Signal Registration Service (Rust Implementation)

A Rust implementation of the Signal Registration Service that handles user registration with LDAP authentication and Twilio verification.

## Features

- LDAP Authentication
- Twilio SMS/Voice Verification
- DynamoDB Storage
- Rate Limiting
- gRPC API
- Configuration Management

## Prerequisites

- Rust (latest stable version)
- Docker (for local DynamoDB)
- LDAP Server
- Twilio Account
- AWS Account (for DynamoDB)

## Configuration

Configuration is managed through YAML files in the `config` directory:

- `application.yml`: Base configuration
- `application-{environment}.yml`: Environment-specific configuration
- `application-local.yml`: Local development overrides (not checked into git)

Environment variables can override any configuration value using the prefix `APP_`.

## Development Setup

1. Clone the repository:
   ```bash
   git clone https://github.com/yourusername/rust_ldap_registration.git
   cd rust_ldap_registration
   ```

2. Copy the example configuration:
   ```bash
   cp config/application.yml config/application-local.yml
   ```

3. Update the configuration with your credentials.

4. Start local DynamoDB:
   ```bash
   ./scripts/start_local_dynamodb.sh
   ```

5. Create DynamoDB table:
   ```bash
   ./scripts/create_dynamodb_table.sh
   ```

6. Build and run:
   ```bash
   cargo run
   ```

## Testing

Run the test suite:
```bash
cargo test
```

## License

Licensed under the AGPLv3 license. See LICENSE file for details.

## Contributing

1. Fork the repository
2. Create your feature branch
3. Commit your changes
4. Push to the branch
5. Create a Pull Request
