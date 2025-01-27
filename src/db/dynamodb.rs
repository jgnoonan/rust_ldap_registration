/// DynamoDB Storage Module
///
/// Provides DynamoDB integration for the Signal Registration Service. This module
/// handles storing and retrieving user registration data in AWS DynamoDB tables.
///
/// # Features
/// - Async DynamoDB operations
/// - Connection pooling via AWS SDK
/// - Local DynamoDB support for development
/// - Configurable table names and endpoints
///
/// # Copyright
/// Copyright (c) 2025 Signal Messenger, LLC
/// All rights reserved.
///
/// # License
/// Licensed under the AGPLv3 license.

use aws_sdk_dynamodb::Client;
use aws_config::Region;
use anyhow::Result;
use tracing::info;
use std::time::SystemTime;
use serde::{Deserialize, Serialize};

/// Configuration for DynamoDB connection and table settings
#[derive(Debug, Clone)]
pub struct DynamoDbConfig {
    /// Name of the DynamoDB table
    pub table_name: String,
    /// AWS region (e.g., "us-west-2")
    pub region: String,
    /// Optional endpoint URL for local development
    pub endpoint: Option<String>,
}

/// Represents a user registration record in DynamoDB
#[derive(Debug, Serialize, Deserialize)]
pub struct RegistrationRecord {
    /// User's username
    pub username: String,
    /// User's phone number (primary key)
    pub phone_number: String,
    /// Signal registration ID
    pub registration_id: String,
    /// Registration timestamp (SystemTime)
    pub created_at: SystemTime,
}

/// Client for DynamoDB operations
#[derive(Debug)]
pub struct DynamoDbClient {
    /// AWS DynamoDB client
    client: Client,
    /// Client configuration
    table_name: String,
}

impl DynamoDbClient {
    /// Creates a new DynamoDB client with the specified configuration
    ///
    /// # Arguments
    /// * `table_name` - Name of the DynamoDB table
    /// * `region` - AWS region (e.g., "us-west-2")
    ///
    /// # Returns
    /// * `Result<DynamoDbClient>` - New DynamoDB client instance or error if configuration fails
    ///
    /// # Examples
    /// ```no_run
    /// use registration_service::db::dynamodb::{DynamoDbClient};
    ///
    /// let client = DynamoDbClient::new("signal_accounts".to_string(), "us-west-2".to_string()).await.expect("Failed to create DynamoDB client");
    /// ```
    pub async fn new(table_name: String, region: String) -> Result<Self> {
        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(Region::new(region))
            .load()
            .await;
            
        let client = Client::new(&config);
        
        Ok(Self {
            client,
            table_name,
        })
    }
    
    /// Stores a new registration record in DynamoDB
    ///
    /// # Arguments
    /// * `record` - Registration record to store
    ///
    /// # Returns
    /// * `Result<()>` - Success or error if storage fails
    ///
    /// # Examples
    /// ```no_run
    /// # use registration_service::db::dynamodb::{DynamoDbClient, RegistrationRecord};
    /// # let client = get_dynamodb_client();
    /// let record = RegistrationRecord {
    ///     username: "username".to_string(),
    ///     phone_number: "+1234567890".to_string(),
    ///     registration_id: "registration_id".to_string(),
    ///     created_at: SystemTime::now(),
    /// };
    ///
    /// client.save_registration(record).await?;
    /// # async fn get_dynamodb_client() -> DynamoDbClient { unimplemented!() }
    /// ```
    pub async fn save_registration(&self, record: RegistrationRecord) -> Result<()> {
        let phone_number = record.phone_number.clone();
        
        self.client
            .put_item()
            .table_name(&self.table_name)
            .item("username", aws_sdk_dynamodb::types::AttributeValue::S(record.username))
            .item("phone_number", aws_sdk_dynamodb::types::AttributeValue::S(phone_number.clone()))
            .item("registration_id", aws_sdk_dynamodb::types::AttributeValue::S(record.registration_id))
            .item(
                "created_at",
                aws_sdk_dynamodb::types::AttributeValue::N(
                    record
                        .created_at
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                        .to_string(),
                ),
            )
            .send()
            .await?;
            
        info!("Saved registration record for phone number: {}", phone_number);
        Ok(())
    }
    
    /// Retrieves a registration record by phone number
    ///
    /// # Arguments
    /// * `phone_number` - Phone number to look up
    ///
    /// # Returns
    /// * `Result<Option<RegistrationRecord>>` - Registration record if found
    ///
    /// # Examples
    /// ```no_run
    /// # use registration_service::db::dynamodb::DynamoDbClient;
    /// # let client = get_dynamodb_client();
    /// match client.get_registration("+1234567890").await? {
    ///     Some(record) => println!("Found registration: {:?}", record),
    ///     None => println!("No registration found"),
    /// }
    /// # async fn get_dynamodb_client() -> DynamoDbClient { unimplemented!() }
    /// ```
    pub async fn get_registration(&self, phone_number: &str) -> Result<Option<RegistrationRecord>> {
        let result = self.client
            .get_item()
            .table_name(&self.table_name)
            .key(
                "phone_number",
                aws_sdk_dynamodb::types::AttributeValue::S(phone_number.to_string()),
            )
            .send()
            .await?;
            
        if let Some(item) = result.item {
            let record = RegistrationRecord {
                username: item.get("username")
                    .and_then(|v| v.as_s().ok())
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
                phone_number: item.get("phone_number")
                    .and_then(|v| v.as_s().ok())
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
                registration_id: item.get("registration_id")
                    .and_then(|v| v.as_s().ok())
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
                created_at: SystemTime::now(),
            };
            Ok(Some(record))
        } else {
            Ok(None)
        }
    }
    
    /// Deletes a registration record by phone number
    ///
    /// # Arguments
    /// * `phone_number` - Phone number of the record to delete
    ///
    /// # Returns
    /// * `Result<()>` - Success or error if deletion fails
    ///
    /// # Examples
    /// ```no_run
    /// # use registration_service::db::dynamodb::DynamoDbClient;
    /// # let client = get_dynamodb_client();
    /// client.delete_registration("+1234567890").await?;
    /// # async fn get_dynamodb_client() -> DynamoDbClient { unimplemented!() }
    /// ```
    pub async fn delete_registration(&self, phone_number: &str) -> Result<()> {
        self.client
            .delete_item()
            .table_name(&self.table_name)
            .key(
                "phone_number",
                aws_sdk_dynamodb::types::AttributeValue::S(phone_number.to_string()),
            )
            .send()
            .await?;
            
        info!("Deleted registration for phone number: {}", phone_number);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_dynamodb_operations() {
        // Add tests here
    }
}
