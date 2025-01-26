use aws_config::meta::region::RegionProviderChain;
use aws_sdk_dynamodb::{Client, Region};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use anyhow::Result;
use tracing::{info, error};

#[derive(Debug, Clone)]
pub struct DynamoDbConfig {
    pub table_name: String,
    pub region: String,
    pub endpoint: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegistrationRecord {
    pub phone_number: String,
    pub registration_id: u64,
    pub device_id: i32,
    pub identity_key: String,
    pub timestamp: i64,
}

pub struct DynamoDbClient {
    client: Client,
    config: DynamoDbConfig,
}

impl DynamoDbClient {
    pub async fn new(config: DynamoDbConfig) -> Result<Self> {
        let region_provider = RegionProviderChain::first_try(Region::new(config.region.clone()));
        
        let mut builder = aws_config::from_env()
            .region(region_provider);
            
        // For local development
        if let Some(endpoint) = &config.endpoint {
            builder = builder.endpoint_url(endpoint);
        }
        
        let sdk_config = builder.load().await;
        let client = Client::new(&sdk_config);
        
        Ok(Self { client, config })
    }
    
    pub async fn store_registration(&self, record: RegistrationRecord) -> Result<()> {
        let mut item = HashMap::new();
        
        item.insert(
            "phone_number".to_string(),
            aws_sdk_dynamodb::types::AttributeValue::S(record.phone_number),
        );
        item.insert(
            "registration_id".to_string(),
            aws_sdk_dynamodb::types::AttributeValue::N(record.registration_id.to_string()),
        );
        item.insert(
            "device_id".to_string(),
            aws_sdk_dynamodb::types::AttributeValue::N(record.device_id.to_string()),
        );
        item.insert(
            "identity_key".to_string(),
            aws_sdk_dynamodb::types::AttributeValue::S(record.identity_key),
        );
        item.insert(
            "timestamp".to_string(),
            aws_sdk_dynamodb::types::AttributeValue::N(record.timestamp.to_string()),
        );
        
        self.client
            .put_item()
            .table_name(&self.config.table_name)
            .set_item(Some(item))
            .send()
            .await?;
            
        info!("Stored registration for phone number: {}", record.phone_number);
        Ok(())
    }
    
    pub async fn get_registration(&self, phone_number: &str) -> Result<Option<RegistrationRecord>> {
        let result = self.client
            .get_item()
            .table_name(&self.config.table_name)
            .key(
                "phone_number",
                aws_sdk_dynamodb::types::AttributeValue::S(phone_number.to_string()),
            )
            .send()
            .await?;
            
        if let Some(item) = result.item {
            let record = RegistrationRecord {
                phone_number: item.get("phone_number")
                    .and_then(|v| v.as_s().ok())
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
                registration_id: item.get("registration_id")
                    .and_then(|v| v.as_n().ok())
                    .and_then(|n| n.parse().ok())
                    .unwrap_or_default(),
                device_id: item.get("device_id")
                    .and_then(|v| v.as_n().ok())
                    .and_then(|n| n.parse().ok())
                    .unwrap_or_default(),
                identity_key: item.get("identity_key")
                    .and_then(|v| v.as_s().ok())
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
                timestamp: item.get("timestamp")
                    .and_then(|v| v.as_n().ok())
                    .and_then(|n| n.parse().ok())
                    .unwrap_or_default(),
            };
            Ok(Some(record))
        } else {
            Ok(None)
        }
    }
    
    pub async fn delete_registration(&self, phone_number: &str) -> Result<()> {
        self.client
            .delete_item()
            .table_name(&self.config.table_name)
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
