/// FIFO buffer
///
/// This implements a simple FIFO buffer which can be useful when managing data to/from Anchor
/// protocol handling. Using this is completely optional, it is provided as a convenience.
pub struct FifoBuffer<const BUF_SIZE: usize> {
    buffer: [u8; BUF_SIZE],
    used: usize,
}

impl<const BUF_SIZE: usize> FifoBuffer<BUF_SIZE> {
    /// Creates a new buffer
    ///
    /// This is declared const, allowing it to be used even in `static const` contexts.
    pub const fn new() -> Self {
        FifoBuffer {
            buffer: [0u8; BUF_SIZE],
            used: 0,
        }
    }

    /// Checks for buffer emptiness
    pub fn is_empty(&self) -> bool {
        self.used == 0
    }

    /// Return length of currently stored buffer
    pub fn len(&self) -> usize {
        self.used
    }

    /// Return mutable slice to the non-filled part of the buffer
    pub fn receive_buffer(&mut self) -> &mut [u8] {
        &mut self.buffer[self.used..]
    }

    /// Append `buf` to the non-filled part of the buffer
    ///
    /// Any excess will be discarded.
    pub fn extend(&mut self, buf: &[u8]) {
        let into = self.receive_buffer();
        if into.len() < buf.len() {
            // Drop if we'd overrun
            return;
        }
        into[..buf.len()].copy_from_slice(buf);
        self.used += buf.len();
    }

    /// Moves the used cursor forward
    ///
    /// This can be used after filling part of the non-filled buffer returned by `receive_buffer`.
    pub fn advance(&mut self, n: usize) {
        self.used = (self.used + n).clamp(0, self.buffer.len());
    }

    /// Returns the filled part of the buffer
    pub fn data(&self) -> &[u8] {
        &self.buffer[0..self.used]
    }

    /// Removes `n` bytes from the front of the buffer
    ///
    /// This operation moves the used part of the buffer down in memory. This is linear in the
    /// number of bytes currently stored.
    pub fn pop(&mut self, n: usize) {
        let n = n.clamp(0, self.used);
        let remain = n..self.used;
        let len = remain.len();
        self.buffer.copy_within(remain, 0);
        self.used = len;
    }
}
