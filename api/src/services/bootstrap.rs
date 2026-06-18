use crate::infra::state::AppState;

pub async fn bootstrap(state: &AppState) {
    let mempool_retriever = state.mempool_retriever.clone();
    let _txs = mempool_retriever
        .get_raw_mempool_verbose()
        .await
        .expect("failed to get mempool");

    // TODO: delta between our db state and the current mempool state
    // TODO: store added/removed txs in the db
    // TODO: create txs that are in mempool but not in db
    // TODO: make mempool cluster call for those who have ancestors or descendants
}
