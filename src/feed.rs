use super::*;

pub const FEED_TAG: u8 = 0x05;

#[derive(Clone, Debug)]
pub struct Event {
    pub topic: Addr,
    pub from: Option<Addr>,
    pub data: Vec<u8>,
}
