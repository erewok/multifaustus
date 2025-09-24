pub mod printer;
use crate::messages;

pub trait Transport {
    fn send(&self, message: &messages::SendableMessage);
}
