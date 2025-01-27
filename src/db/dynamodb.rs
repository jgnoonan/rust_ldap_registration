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

/// Represents a user registration record in DynamoDB
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

#[derive(Debug)]
pub struct DynamoDbClient {
    client: Box<dyn DynamoDbOps>,
    config: DynamoDbConfig,
}

impl DynamoDbClient {
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
