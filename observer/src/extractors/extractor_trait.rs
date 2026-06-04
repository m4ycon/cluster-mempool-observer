use std::fmt::Debug;

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

pub trait Extractor<Response: PartialEq, Event: Debug> {
    /// Extracts a response from the source
    fn extract(&mut self) -> impl Future<Output = Result<Response, ExtractorError>> + Send;

    /// Says if the extractor can extract again, the extractor can have an
    /// internal state that makes it unable to extract again.
    fn can_extract_again(&self) -> bool;

    /// Updates the last response, returns true if the response
    /// is different from the last one.
    fn update_last_response(&mut self, response: &Response) -> bool;

    /// Transforms the response into an event. Usually will reduce the response
    /// to a smaller set of data, or transform it into a different format.
    fn into_event(&mut self, response: &Response) -> Event;
}
