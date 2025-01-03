use crate::{ApiError, BroadcastOperation};
use aws_sdk_sns::{Client as SnsClient, Error as SnsError};
use log::{error, info};
use rocket::fairing::AdHoc;
use rocket::tokio;
use rocket::tokio::sync::Mutex;
use std::sync::Arc;
use tokio_postgres::{Client, NoTls};

/// Fairing for managing the PostgreSQL client in rocket's state
pub fn attatch_db() -> AdHoc {
    AdHoc::on_ignite("Attatch DB", |rocket| async {
        match connect_to_db().await {
            Ok(client) => rocket.manage(Arc::new(Mutex::new(client))),
            Err(e) => {
                error!(
                    "Unable to start server, failed to initialize database: {}",
                    e
                );
                eprintln!("Failed to initialize DB: {:?}", e);
                std::process::exit(1);
            }
        }
    })
}

/// Connects to the AWS RDS instance using the database connection url set in the .env file under
/// DB_URL
pub async fn connect_to_db() -> Result<Client, ApiError> {
    let database_url = match std::env::var("DB_URL") {
        Ok(url) => url,
        Err(_) => {
            error!("DB_URL not set in the .env file");
            std::process::exit(1);
        }
    };

    let (client, connection) = tokio_postgres::connect(&database_url, NoTls)
        .await
        .map_err(|e| {
            error!("Failed to establish database connection.");
            ApiError::DatabaseError(e.to_string())
        })?;

    tokio::spawn(async move { connection.await });
    info!("Successfully established a connection to the database");
    return Ok(client);
}

/// Sends a SNS message
pub async fn send_sns_notification(
    message: &str,
    sns_topic: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = aws_config::load_from_env().await;
    let sns_client = aws_sdk_sns::Client::new(&config);

    match sns_client
        .publish()
        .message(message)
        .topic_arn(sns_topic)
        .send()
        .await
    {
        Ok(_) => {
            info!("SNS notification sent to other replicas");
            Ok(())
        }
        Err(e) => {
            error!("Failed to send SNS notification to other replicas");
            Err(Box::new(e))
        }
    }
}

pub async fn send_operation(
    sns_client: Arc<Mutex<SnsClient>>,
    topic_arn: &str,
    operation: &BroadcastOperation,
) -> Result<(), SnsError> {
    let message = serde_json::to_string(operation).expect("Failed to serialize operation");
    sns_client
        .lock()
        .await
        .publish()
        .topic_arn(topic_arn)
        .message(message)
        .send()
        .await?;

    info!(
        "SNS {} operation sent to other replicas",
        operation.operation
    );
    println!("Operation sent to other replicas");
    return Ok(());
}
