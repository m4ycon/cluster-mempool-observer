use crate::services::nats::NatsService;
use axum::Router;
use axum::extract::FromRef;

#[derive(Clone)]
pub struct AppState {
    pub nats: NatsService,
}

impl AppState {
    pub fn new(nats: NatsService) -> Self {
        Self { nats }
    }
}

impl FromRef<AppState> for NatsService {
    fn from_ref(state: &AppState) -> Self {
        state.nats.clone()
    }
}

pub type AppRouter = Router<AppState>;
