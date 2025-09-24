use tracing::info;

use crate::messages;
use crate::transport::Transport;

pub struct Printer;

impl Transport for Printer {
    fn send(&self, message: &messages::SendableMessage) {
        info!("sending message [{}]", message);
    }
}
