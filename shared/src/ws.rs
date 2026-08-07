use crate::TS_EXPORT_DIR;
use crate::events::{ClusterDeltaEvent, MempoolDeltaEvent, MempoolStatsEvent, NewBlockInfoEvent};
use serde::{Deserialize, Serialize};
use std::fmt;
use ts_rs::TS;

/// Subjects a client can subscribe/unsubscribe to on the multiplexed `/ws` route.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub enum WsSubject {
    #[serde(rename = "mempool.delta")]
    MempoolDelta,
    #[serde(rename = "mempool.stats")]
    MempoolStats,
    #[serde(rename = "chain.tip")]
    ChainTip,
    #[serde(rename = "cluster.delta")]
    ClusterDelta,
}

impl WsSubject {
    pub fn as_str(&self) -> &'static str {
        match self {
            WsSubject::MempoolDelta => "mempool.delta",
            WsSubject::MempoolStats => "mempool.stats",
            WsSubject::ChainTip => "chain.tip",
            WsSubject::ClusterDelta => "cluster.delta",
        }
    }
}

impl fmt::Display for WsSubject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Client -> server frames on the multiplexed `/ws` route.
#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(tag = "action", rename_all = "snake_case")]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub enum ClientFrame {
    Subscribe { subject: WsSubject },
    Unsubscribe { subject: WsSubject },
}

/// Server -> client frames on the multiplexed `/ws` route.
#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(tag = "subject", content = "payload")]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub enum ServerEvent {
    #[serde(rename = "mempool.delta")]
    MempoolDelta(MempoolDeltaEvent),
    #[serde(rename = "mempool.stats")]
    MempoolStats(MempoolStatsEvent),
    #[serde(rename = "chain.tip")]
    ChainTip(NewBlockInfoEvent),
    #[serde(rename = "cluster.delta")]
    ClusterDelta(ClusterDeltaEvent),
    #[serde(rename = "error")]
    Error(WsError),
}

/// An error frame sent to the client over the multiplexed `/ws` route.
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct WsError {
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_frame_subscribe_deserializes_for_each_subject() {
        let cases = [
            ("mempool.delta", WsSubject::MempoolDelta),
            ("mempool.stats", WsSubject::MempoolStats),
            ("chain.tip", WsSubject::ChainTip),
            ("cluster.delta", WsSubject::ClusterDelta),
        ];

        for (wire_name, expected) in cases {
            let json = format!(r#"{{"action":"subscribe","subject":"{wire_name}"}}"#);
            let frame: ClientFrame = serde_json::from_str(&json).unwrap();
            match frame {
                ClientFrame::Subscribe { subject } => assert_eq!(subject, expected),
                ClientFrame::Unsubscribe { .. } => panic!("expected Subscribe"),
            }
        }
    }

    #[test]
    fn client_frame_unknown_subject_fails_to_deserialize() {
        let json = r#"{"action":"subscribe","subject":"not.a.subject"}"#;
        let result: Result<ClientFrame, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn server_event_cluster_delta_serializes_to_subject_and_payload() {
        let event = ServerEvent::ClusterDelta(ClusterDeltaEvent {
            upserted: Vec::new(),
            removed: Vec::new(),
        });
        let value = serde_json::to_value(&event).unwrap();
        let obj = value.as_object().unwrap();
        assert_eq!(obj.len(), 2);
        assert_eq!(obj.get("subject").unwrap(), "cluster.delta");
        assert!(obj.contains_key("payload"));
    }

    #[test]
    fn server_event_error_serializes_to_subject_and_payload() {
        let event = ServerEvent::Error(WsError {
            message: "boom".to_string(),
        });
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"subject": "error", "payload": {"message": "boom"}})
        );
    }

    #[test]
    fn ws_subject_as_str_matches_serde_wire_name() {
        let subjects = [
            WsSubject::MempoolDelta,
            WsSubject::MempoolStats,
            WsSubject::ChainTip,
            WsSubject::ClusterDelta,
        ];

        for subject in subjects {
            let serialized = serde_json::to_string(&subject).unwrap();
            let expected = format!("\"{}\"", subject.as_str());
            assert_eq!(serialized, expected);
        }
    }
}
