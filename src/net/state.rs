use std::time::Instant;

pub struct State {
    pub last_user_facing_request: Instant,
    pub last_background_upload: Instant,
}
