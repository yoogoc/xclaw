pub struct Thread {

}

#[derive(Clone, Hash, Eq, PartialEq)]
pub struct ThreadKey {
    user_id: String,
    // channel: String,
    external_thread_id: Option<String>,
}