use crate::output_buffer::OutputBuffer;

/// Error type for representing a failed read
pub struct ReadError;

/// Trait implemented for types that can be read from an input message
///
/// The `'de` lifetime allows the implementation to return references to the original data buffer.
/// This permits zero-copy reading of variable length data like byte arrays.
pub trait Readable<'de>: Sized {
    /// Attempt to read a `Self` from the input buffer, advancing the buffer if successful.
    ///
    /// If the operaetion fails, `data` should not be advanced and a `ReadError` should be
    /// returned.
    fn read(data: &mut &'de [u8]) -> Result<Self, ReadError>;
}

pub(crate) fn next_byte(data: &mut &[u8]) -> Result<u8, ReadError> {
    if data.is_empty() {
        Err(ReadError)
    } else {
        let v = data[0];
        *data = &data[1..];
        Ok(v)
    }
}

fn parse_vlq_int(data: &mut &[u8]) -> Result<u32, ReadError> {
    let mut c = next_byte(data)? as u32;
    let mut v = c & 0x7F;
    if (c & 0x60) == 0x60 {
        v |= (-0x20i32) as u32;
    }
    while c & 0x80 != 0 {
        c = next_byte(data)? as u32;
        v = (v << 7) | (c & 0x7F);
    }

    Ok(v)
}

/// Trait implemented for types that can be written to an `OutputBuffer`
pub trait Writable: Sized {
    /// Outputs the type to an `OutputBuffer`
    ///
    /// This operation cannot fail, and `OutputBuffer` has no way to indicate an overfull buffer.
    /// The buffer will simply be truncated and the final message will be invalid, likely causing
    /// the remote end to error out. It is up to the user to avoid this situation.
    fn write(&self, output: &mut impl OutputBuffer);
}

fn encode_vlq_int(output: &mut impl OutputBuffer, v: u32) {
    let sv = v as i32;
    if !(-(1 << 26)..(3 << 26)).contains(&sv) {
        output.output(&[((sv >> 28) & 0x7F) as u8 | 0x80]);
    }
    if !(-(1 << 19)..(3 << 19)).contains(&sv) {
        output.output(&[((sv >> 21) & 0x7F) as u8 | 0x80]);
    }
    if !(-(1 << 12)..(3 << 12)).contains(&sv) {
        output.output(&[((sv >> 14) & 0x7F) as u8 | 0x80]);
    }
    if !(-(1 << 5)..(3 << 5)).contains(&sv) {
        output.output(&[((sv >> 7) & 0x7F) as u8 | 0x80]);
    }
    output.output(&[(sv & 0x7F) as u8]);
}

macro_rules! int_readwrite {
    ( $type:tt ) => {
        impl Readable<'_> for $type {
            fn read(data: &mut &[u8]) -> Result<Self, ReadError> {
                parse_vlq_int(data).map(|v| v as $type)
            }
        }

        impl Writable for $type {
            fn write(&self, output: &mut impl OutputBuffer) {
                encode_vlq_int(output, *self as u32)
            }
        }
    };
}

int_readwrite!(u32);
int_readwrite!(i32);
int_readwrite!(u16);
int_readwrite!(i16);
int_readwrite!(u8);

impl Readable<'_> for bool {
    fn read(data: &mut &[u8]) -> Result<Self, ReadError> {
        parse_vlq_int(data).map(|v| v != 0)
    }
}

impl Writable for bool {
    fn write(&self, output: &mut impl OutputBuffer) {
        encode_vlq_int(output, u32::from(*self))
    }
}

impl<'de> Readable<'de> for &'de [u8] {
    fn read(data: &mut &'de [u8]) -> Result<&'de [u8], ReadError> {
        let len = parse_vlq_int(data)? as usize;
        if data.len() < len {
            Err(ReadError)
        } else {
            let ret = &data[..len];
            *data = &data[len..];
            Ok(ret)
        }
    }
}

impl Writable for &[u8] {
    fn write(&self, output: &mut impl OutputBuffer) {
        encode_vlq_int(output, self.len() as u32);
        output.output(self);
    }
}

impl Writable for &str {
    fn write(&self, output: &mut impl OutputBuffer) {
        let bytes = self.as_bytes();
        encode_vlq_int(output, bytes.len() as u32);
        output.output(bytes);
    }
}
