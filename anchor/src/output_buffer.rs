/// Trait for output buffers that can accept encoded data.
///
/// Message builders accept an argumenet of this type and will output their data in to the buffer.
///
/// The buffer must support simple seeking to a previously retrieved position. This is used when
/// calculating checksums.
pub trait OutputBuffer {
    /// The cursor type
    type Cursor: Copy;
    /// Append bytes to the buffer
    fn output(&mut self, buf: &[u8]);
    /// Retrieve the cursor representing the position of the last appended byte
    fn cur_position(&self) -> Self::Cursor;
    /// Replace the byte at the cursor position with a new value
    fn update(&mut self, cursor: Self::Cursor, value: u8);
    /// Retrieve a reference to all data pushed after the cursor
    fn data_since(&self, cursor: Self::Cursor) -> &[u8];
}

/// A scratch pad based `OutputBuffer`.
///
/// Uses a statically sized inlined buffer. For serializing multiple messages in a row, the buffer
/// can be reset if needed.
pub struct ScratchOutput<const MAX_SIZE: usize = 64> {
    buffer: [u8; MAX_SIZE],
    idx: usize,
}

impl<const MAX_SIZE: usize> ScratchOutput<MAX_SIZE> {
    /// Retrieve the currently built buffer
    pub fn result(&self) -> &[u8] {
        &self.buffer[..self.idx]
    }

    /// Reset the buffer, clearing it
    pub fn reset(&mut self) {
        self.idx = 0;
    }

    /// Create a new buffer
    pub const fn new() -> Self {
        Self {
            buffer: [0u8; MAX_SIZE],
            idx: 0,
        }
    }
}

impl<const MAX_SIZE: usize> OutputBuffer for ScratchOutput<MAX_SIZE> {
    type Cursor = usize;

    fn output(&mut self, buf: &[u8]) {
        let area = &mut self.buffer[self.idx..];
        let len = buf.len().clamp(0, area.len());
        area[..len].copy_from_slice(buf);
        self.idx += len;
    }

    fn cur_position(&self) -> Self::Cursor {
        self.idx
    }

    fn update(&mut self, cursor: Self::Cursor, value: u8) {
        if cursor < self.idx {
            if let Some(b) = self.buffer.get_mut(cursor) {
                *b = value;
            }
        }
    }

    fn data_since(&self, cursor: Self::Cursor) -> &[u8] {
        if cursor >= self.idx {
            &[]
        } else {
            &self.buffer[cursor..self.idx]
        }
    }
}

#[cfg(feature = "std")]
impl OutputBuffer for Vec<u8> {
    type Cursor = usize;

    fn output(&mut self, buf: &[u8]) {
        self.extend(buf)
    }

    fn cur_position(&self) -> Self::Cursor {
        self.len().saturating_sub(1)
    }

    fn update(&mut self, cursor: Self::Cursor, value: u8) {
        self[cursor] = value;
    }

    fn data_since(&self, cursor: Self::Cursor) -> &[u8] {
        &self[cursor..]
    }
}
