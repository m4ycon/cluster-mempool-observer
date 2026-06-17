use crate::services::pubsub::PubSubService;
use axum::Router;
use axum::extract::FromRef;

#[derive(Clone)]
pub struct AppState {
    pub pubsub: PubSubService,
}

impl AppState {
    pub fn new(pubsub: PubSubService) -> Self {
        Self { pubsub }
    }
}

impl FromRef<AppState> for PubSubService {
    fn from_ref(state: &AppState) -> Self {
        state.pubsub.clone()
    }
}

pub type AppRouter = Router<AppState>;
