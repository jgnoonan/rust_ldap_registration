/// Build Script for Signal Registration Service
///
/// This build script handles the compilation of protocol buffer definitions
/// and generation of Rust code for the gRPC service interface.
///
/// # Features
/// - Protocol buffer compilation
/// - gRPC service code generation
/// - Build-time configuration
///
/// # Copyright
/// Copyright (c) 2025 Signal Messenger, LLC
/// All rights reserved.
///
/// # License
/// Licensed under the AGPLv3 license.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile the protocol buffer definitions
    tonic_build::compile_protos("proto/registration.proto")?;
    Ok(())
}
