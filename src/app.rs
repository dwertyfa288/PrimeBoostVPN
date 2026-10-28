use std::sync::Arc;

use parking_lot::Mutex;

use crate::core::Core;
use crate::session::{self, StoredSession};
use crate::state::State;

#[derive(Default)]
pub struct App {
    pub state: Mutex<State>,
    pub core: Core,
}

pub type SharedApp = Arc<App>;

impl App {
    pub fn persist_session(&self) {
        let state = self.state.lock();
        if let Some(token) = &state.token {
            session::save(&StoredSession {
                token: token.clone(),
                email: state.email.clone(),
            });
        }
    }
}
