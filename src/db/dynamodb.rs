//! DynamoDB client implementation for persistent storage.
//!
//! This module provides a DynamoDB-based implementation for storing and retrieving
//! user registration records. It handles the persistence layer of the registration
//! service, maintaining a record of registered users and their associated data.
//!
//! @author Joseph G Noonan
//! @copyright 2025
use aws_sdk_dynamodb::Client as AwsDynamoDbClient;
use aws_sdk_dynamodb::error::SdkError;
use aws_sdk_dynamodb::operation::delete_item::DeleteItemError;
use aws_sdk_dynamodb::operation::get_item::GetItemError;
use aws_sdk_dynamodb::operation::put_item::PutItemError;
use aws_sdk_dynamodb::types::AttributeValue;
use aws_config::meta::region::RegionProviderChain;
use aws_config::Region;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use thiserror::Error;
use tracing::{info, error};

/// Configuration for DynamoDB connection and table settings
#[derive(Debug, Clone)]
pub struct DynamoDbConfig {
    /// AWS region (e.g., "us-west-2")
    pub region: String,
    /// DynamoDB table name
    pub table_name: String,
}

/// Represents a user registration record in DynamoDB.
#[derive(Debug, Serialize, Deserialize)]
pub struct RegistrationRecord {
    /// User's username
    pub username: String,
    /// User's phone number (primary key)
    pub phone_number: String,
    /// Signal registration ID
    pub registration_id: String,
}

#[async_trait::async_trait]
pub trait DynamoDbOps: std::fmt::Debug + Send + Sync {
    async fn put_item(
        &self,
        input: aws_sdk_dynamodb::operation::put_item::PutItemInput,
    ) -> Result<
        aws_sdk_dynamodb::operation::put_item::PutItemOutput,
        SdkError<PutItemError>,
    >;

    async fn get_item(
        &self,
        input: aws_sdk_dynamodb::operation::get_item::GetItemInput,
    ) -> Result<
        aws_sdk_dynamodb::operation::get_item::GetItemOutput,
        SdkError<GetItemError>,
    >;

    async fn delete_item(
        &self,
        input: aws_sdk_dynamodb::operation::delete_item::DeleteItemInput,
    ) -> Result<
        aws_sdk_dynamodb::operation::delete_item::DeleteItemOutput,
        SdkError<DeleteItemError>,
    >;
}

#[async_trait::async_trait]
impl DynamoDbOps for AwsDynamoDbClient {
    async fn put_item(
        &self,
        input: aws_sdk_dynamodb::operation::put_item::PutItemInput,
    ) -> Result<
        aws_sdk_dynamodb::operation::put_item::PutItemOutput,
        SdkError<PutItemError>,
    > {
        self.put_item()
            .set_item(input.item().cloned())
            .set_table_name(input.table_name().map(|s| s.to_string()))
            .send()
            .await
    }

    async fn get_item(
        &self,
        input: aws_sdk_dynamodb::operation::get_item::GetItemInput,
    ) -> Result<
        aws_sdk_dynamodb::operation::get_item::GetItemOutput,
        SdkError<GetItemError>,
    > {
        self.get_item()
            .set_key(input.key().cloned())
            .set_table_name(input.table_name().map(|s| s.to_string()))
            .send()
            .await
    }

    async fn delete_item(
        &self,
        input: aws_sdk_dynamodb::operation::delete_item::DeleteItemInput,
    ) -> Result<
        aws_sdk_dynamodb::operation::delete_item::DeleteItemOutput,
        SdkError<DeleteItemError>,
    > {
        self.delete_item()
            .set_key(input.key().cloned())
            .set_table_name(input.table_name().map(|s| s.to_string()))
            .send()
            .await
    }
}

/// Client for interacting with DynamoDB registration table.
///
/// Provides methods for storing, retrieving, and managing user registration
/// records in DynamoDB. The client handles all AWS SDK interactions and
/// provides a high-level interface for registration operations.
pub struct DynamoDbClient {
    client: Box<dyn DynamoDbOps>,
    config: DynamoDbConfig,
}

impl DynamoDbClient {
    /// Creates a new DynamoDB client instance.
    ///
    /// # Arguments
    /// * `table_name` - Name of the DynamoDB table for registrations
    /// * `region` - AWS region for the DynamoDB table
    ///
    /// # Returns
    /// * `Result<Self>` - New client instance or error if initialization fails
    pub async fn new(table_name: String, region: String) -> Result<Self, Error> {
        let region_provider = RegionProviderChain::first_try(Region::new(region.clone()));
        let shared_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(region_provider)
            .load()
            .await;
        let client = AwsDynamoDbClient::new(&shared_config);

        Ok(Self {
            client: Box::new(client),
            config: DynamoDbConfig {
                region,
                table_name,
            },
        })
    }

    /// Stores a new registration record in DynamoDB.
    ///
    /// # Arguments
    /// * `username` - Username associated with the registration
    /// * `phone_number` - User's verified phone number
    /// * `registration_id` - Signal registration ID
    ///
    /// # Returns
    /// * `Result<()>` - Success or error if storage fails
    pub async fn save_registration(
        &self,
        username: &str,
        phone_number: &str,
        registration_id: &str,
    ) -> Result<(), Error> {
        let mut item = HashMap::new();
        item.insert(
            "phone_number".to_string(),
            AttributeValue::S(phone_number.to_string()),
        );
        item.insert(
            "username".to_string(),
            AttributeValue::S(username.to_string()),
        );
        item.insert(
            "registration_id".to_string(),
            AttributeValue::S(registration_id.to_string()),
        );

        let input = aws_sdk_dynamodb::operation::put_item::PutItemInput::builder()
            .table_name(&self.config.table_name)
            .set_item(Some(item))
            .build()
            .map_err(Error::BuildError)?;

        self.client
            .put_item(input)
            .await
            .map_err(Error::PutItemError)?;

        info!("Saved registration for phone number: {}", phone_number);
        Ok(())
    }

    /// Retrieves a registration record by phone number.
    ///
    /// # Arguments
    /// * `phone_number` - Phone number to look up
    ///
    /// # Returns
    /// * `Result<Option<RegistrationRecord>>` - Registration record if found
    pub async fn get_registration(
        &self,
        phone_number: &str,
    ) -> Result<Option<RegistrationRecord>, Error> {
        let mut key = HashMap::new();
        key.insert(
            "phone_number".to_string(),
            AttributeValue::S(phone_number.to_string()),
        );

        let input = aws_sdk_dynamodb::operation::get_item::GetItemInput::builder()
            .table_name(&self.config.table_name)
            .set_key(Some(key))
            .build()
            .map_err(Error::BuildError)?;

        let output = self.client
            .get_item(input)
            .await
            .map_err(Error::GetItemError)?;

        if let Some(item) = output.item {
            let username = item
                .get("username")
                .and_then(|av| av.as_s().ok())
                .ok_or_else(|| Error::ParseError("username".to_string()))?
                .to_string();

            let registration_id = item
                .get("registration_id")
                .and_then(|av| av.as_s().ok())
                .ok_or_else(|| Error::ParseError("registration_id".to_string()))?
                .to_string();

            Ok(Some(RegistrationRecord {
                username,
                phone_number: phone_number.to_string(),
                registration_id,
            }))
        } else {
            Ok(None)
        }
    }

    /// Deletes a registration record by phone number.
    ///
    /// # Arguments
    /// * `phone_number` - Phone number of the record to delete
    ///
    /// # Returns
    /// * `Result<()>` - Success or error if deletion fails
    pub async fn delete_registration(&self, phone_number: &str) -> Result<(), Error> {
        let mut key = HashMap::new();
        key.insert(
            "phone_number".to_string(),
            AttributeValue::S(phone_number.to_string()),
        );

        let input = aws_sdk_dynamodb::operation::delete_item::DeleteItemInput::builder()
            .table_name(&self.config.table_name)
            .set_key(Some(key))
            .build()
            .map_err(Error::BuildError)?;

        self.client
            .delete_item(input)
            .await
            .map_err(Error::DeleteItemError)?;

        info!("Deleted registration for phone number: {}", phone_number);
        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to build input: {0}")]
    BuildError(#[from] aws_sdk_dynamodb::error::BuildError),
    #[error("Failed to put item: {0}")]
    PutItemError(SdkError<PutItemError>),
    #[error("Failed to get item: {0}")]
    GetItemError(SdkError<GetItemError>),
    #[error("Failed to delete item: {0}")]
    DeleteItemError(SdkError<DeleteItemError>),
    #[error("Failed to parse {0} from DynamoDB response")]
    ParseError(String),
}
