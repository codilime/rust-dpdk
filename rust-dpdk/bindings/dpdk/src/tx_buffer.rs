use crate::eal::{Packet, TxQ};
use crate::zeroable::Zeroable;
use arrayvec::ArrayVec;

pub struct TxBuffer<'pool, MPoolPriv, const CAP: usize>
where
    MPoolPriv: Zeroable,
{
    buff: ArrayVec<Packet<'pool, MPoolPriv>, CAP>,
}

impl<'pool, MPoolPriv, const CAP: usize> TxBuffer<'pool, MPoolPriv, CAP>
where
    MPoolPriv: Zeroable,
{
    // Create new `TxBuffer`.
    pub fn new() -> Self {
        TxBuffer {
            buff: ArrayVec::new(),
        }
    }

    /// Buffer a single packet for future transmission on a tx queue
    ///
    /// This function takes a single packet and buffers it for later
    /// transmission on the particular port and queue specified. Once the buffer is
    /// full of packets, an attempt will be made to transmit all the buffered
    /// packets.
    /// The function returns the number of packets actually sent and may return
    /// an iterator to packets that couldn't be sent in case of failed flush.
    pub fn tx(
        &mut self,
        txq: &mut TxQ<'pool>,
        pkt: Packet<'pool, MPoolPriv>,
    ) -> (
        usize,
        Option<arrayvec::Drain<'_, Packet<'pool, MPoolPriv>, CAP>>,
    ) {
        self.buff.push(pkt);
        if self.buff.is_full() {
            return self.flush(txq);
        }
        (0, None)
    }

    /// Send any packets queued up for transmission on a tx queue
    ///
    /// This causes an explicit flush of packets previously buffered via the
    /// tx() method. It returns the number of packets successfully
    /// sent to the NIC, and, if there are some unsent packets, returns an
    /// iterator to these packets.
    pub fn flush(
        &mut self,
        txq: &mut TxQ<'pool>,
    ) -> (
        usize,
        Option<arrayvec::Drain<'_, Packet<'pool, MPoolPriv>, CAP>>,
    ) {
        if self.buff.len() == 0 {
            return (0, None);
        }

        let to_send = self.buff.len();
        txq.tx(&mut self.buff);
        let sent = to_send - self.buff.len();

        if self.buff.is_empty() {
            (sent, None)
        } else {
            (sent, Some(self.buff.drain(..)))
        }
    }
}
