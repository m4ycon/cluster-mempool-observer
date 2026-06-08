use std::{
    fmt::Debug,
    time::{Duration, Instant},
};

use futures::future::join_all;
use serde::Serialize;
use tokio::time::sleep;

use crate::{
    extractors::{extractor_trait::Extractor, getrawmempool::GetRawMempoolExtractor},
    publisher::publish_event,
};

pub async fn run(poll_interval_secs: u64) {
    let mut extractors = vec![GetRawMempoolExtractor::default()];
    let poll_interval = Duration::from_secs(poll_interval_secs);

    loop {
        let start = Instant::now();

        let futures = extractors.iter_mut().map(|e| execute_extractor(e));
        join_all(futures).await;

        let elapsed = start.elapsed();
        if elapsed < poll_interval {
            sleep(poll_interval - elapsed).await;
        }
    }
}

async fn execute_extractor<Response, Event, T>(extractor: &mut T)
where
    Response: PartialEq,
    Event: Debug + Serialize,
    T: Extractor<Response, Event>,
{
    if !extractor.can_extract_again() {
        return;
    }

    let res = match extractor.extract().await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Error extracting: {:?}", e);
            return;
        }
    };

    if !extractor.update_last_response(&res) {
        return;
    }

    let subject = extractor.subject();
    let event = extractor.into_event(&res);

    publish_event(subject, &event).await;
}
