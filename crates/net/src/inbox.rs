//! Per channel, the packets `receive` has not taken.

use crate::{PeerId, RELIABLE, UNRELIABLE};
use std::collections::VecDeque;

/// Most packets, and bytes, one channel's inbox holds.
pub(crate) const INBOX_PACKETS: usize = 8192;
pub(crate) const INBOX_BYTES: usize = 8 << 20;

#[derive(Default)]
pub(crate) struct Inbox {
    packets: [VecDeque<(PeerId, Vec<u8>)>; 2],
    bytes: [usize; 2],
}

impl Inbox {
    /// Queues a packet. A full unreliable inbox drops its oldest packets; a full reliable one refuses it and returns
    /// false, and the caller drops the peer.
    pub(crate) fn deliver(&mut self, channel: usize, peer: PeerId, data: Vec<u8>) -> bool {
        let full = |s: &Inbox| s.packets[channel].len() >= INBOX_PACKETS || s.bytes[channel] + data.len() > INBOX_BYTES;
        if channel == RELIABLE && full(self) {
            return false;
        }
        while full(self) {
            let Some((_, old)) = self.packets[channel].pop_front() else { break };
            self.bytes[channel] -= old.len();
        }
        self.bytes[channel] += data.len();
        self.packets[channel].push_back((peer, data));
        true
    }

    pub(crate) fn take(&mut self, channel: usize) -> Vec<(PeerId, Vec<u8>)> {
        if channel > UNRELIABLE {
            return Vec::new();
        }
        self.bytes[channel] = 0;
        std::mem::take(&mut self.packets[channel]).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_unreliable_inbox_drops_its_oldest_packets() {
        let mut inbox = Inbox::default();
        for i in 0..INBOX_PACKETS as u32 + 2 {
            assert!(inbox.deliver(UNRELIABLE, PeerId(1), i.to_le_bytes().to_vec()));
        }
        let packets = inbox.take(UNRELIABLE);
        assert_eq!(packets.len(), INBOX_PACKETS);
        assert_eq!(packets[0].1, 2u32.to_le_bytes());

        assert!(inbox.deliver(UNRELIABLE, PeerId(1), vec![0; INBOX_BYTES]));
        assert!(inbox.deliver(UNRELIABLE, PeerId(1), vec![1]));
        assert_eq!(inbox.take(UNRELIABLE), vec![(PeerId(1), vec![1])]);
    }

    #[test]
    fn a_full_reliable_inbox_refuses_more() {
        let mut inbox = Inbox::default();
        assert!(inbox.deliver(RELIABLE, PeerId(1), vec![0; INBOX_BYTES - 1]));
        assert!(inbox.deliver(RELIABLE, PeerId(1), vec![0]));
        assert!(!inbox.deliver(RELIABLE, PeerId(2), vec![0]));
        assert_eq!(inbox.take(RELIABLE).len(), 2);
        assert!(inbox.deliver(RELIABLE, PeerId(2), vec![0]));
    }

    #[test]
    fn channels_keep_their_own_packets() {
        let mut inbox = Inbox::default();
        assert!(inbox.deliver(RELIABLE, PeerId(1), vec![1]));
        assert!(inbox.deliver(UNRELIABLE, PeerId(2), vec![2]));
        assert_eq!(inbox.take(UNRELIABLE), vec![(PeerId(2), vec![2])]);
        assert_eq!(inbox.take(RELIABLE), vec![(PeerId(1), vec![1])]);
        assert!(inbox.take(RELIABLE).is_empty());
        assert!(inbox.take(2).is_empty());
    }
}
