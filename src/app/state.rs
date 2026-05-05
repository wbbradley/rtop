use std::sync::Arc;

use crate::process::Snapshot;

#[derive(Default)]
pub struct App {
    pub latest: Option<Arc<Snapshot>>,
    pub quit: bool,
}
