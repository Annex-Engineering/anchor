/// Trait representing a buffer that protocol messages can be read from
pub trait InputBuffer {
    /// Retrieve all current data in the buffer
    fn data(&self) -> &[u8];
    /// Remove `count` bytes from the front of the buffer
    fn pop(&mut self, count: usize);
    /// Retrieve the amount of data currently in the buffer
    fn available(&self) -> usize {
        self.data().len()
    }
}

/// An `InputBuffer` implementation wrapping a slice
pub struct SliceInputBuffer<'a> {
    buffer: &'a [u8],
}

impl<'a> SliceInputBuffer<'a> {
    /// Create a new `SliceInputBuffer` backed by an input byte slice
    pub fn new(buffer: &'a [u8]) -> Self {
        Self { buffer }
    }
}

impl<'a> InputBuffer for SliceInputBuffer<'a> {
    fn data(&self) -> &[u8] {
        self.buffer
    }

    fn pop(&mut self, count: usize) {
        let count = count.clamp(0, self.buffer.len());
        self.buffer = &self.buffer[count..];
    }
}

#[cfg(feature = "std")]
impl InputBuffer for Vec<u8> {
    fn data(&self) -> &[u8] {
        &self[..]
    }

    fn pop(&mut self, count: usize) {
        self.splice(0..count, std::iter::empty());
    }

    fn available(&self) -> usize {
        self.len()
    }
}
