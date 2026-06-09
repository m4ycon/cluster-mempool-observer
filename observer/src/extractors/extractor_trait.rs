use crate::infra::{config::ExtractorsConfig, nats::Subject};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::{fmt::Debug, time::Instant};

#[derive(Debug)]
pub enum ExtractorError {
    FailedToConnect(String),
    FailedToExtract(String),
    Other(String),
}

impl From<std::io::Error> for ExtractorError {
    fn from(err: std::io::Error) -> Self {
        ExtractorError::Other(err.to_string())
    }
}

pub trait Extractor: Send {
    type Response: PartialEq + Send;
    type Event: Debug + Serialize + DeserializeOwned + Send + Sync;

    /// The event's subject from this extractor are published to.
    fn subject(&self) -> Subject;

    /// Extracts a response from the source
    fn extract(&mut self) -> impl Future<Output = Result<Self::Response, ExtractorError>> + Send;

    /// Says if the extractor can extract again, the extractor can have an
    /// internal state that makes it unable to extract again.
    fn can_extract_again(&self) -> bool;

    /// Updates the last response, returns true if the response
    /// is different from the last one.
    fn update_last_response(&mut self, response: &Self::Response) -> bool;

    /// Transforms the response into an event. Usually will reduce the response
    /// to a smaller set of data, or transform it into a different format.
    fn to_event(&self, response: &Self::Response) -> Self::Event;

    /// Says if the extractor is enabled by config
    fn is_enabled(&self, config: &ExtractorsConfig) -> bool;

    /// Executes the extractor default workflow, extracting data, checking if
    /// it changed, and publishing the resulting event if so.
    fn run_once<F, Fut>(&mut self, publish_event: F) -> impl Future<Output = ()> + Send
    where
        F: FnOnce(Subject, Self::Event) -> Fut + Send,
        Fut: Future<Output = ()> + Send,
    {
        async move {
            if !self.can_extract_again() {
                return;
            }

            let start = Instant::now();
            let res = match self.extract().await {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!("Error extracting: {:?}", e);
                    return;
                }
            };
            tracing::debug!(
                "{} extractor call took {:?}",
                self.subject(),
                start.elapsed()
            );

            if !self.update_last_response(&res) {
                return;
            }

            let subject = self.subject();
            let event = self.to_event(&res);

            let start = Instant::now();
            publish_event(subject, event).await;
            tracing::debug!("{} publish took {:?}", self.subject(), start.elapsed());
        }
    }
}
