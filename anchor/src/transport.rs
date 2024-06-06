use crate::encoding::*;
use crate::input_buffer::InputBuffer;
use crate::output_buffer::OutputBuffer;
use crate::transport_output::TransportOutput;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

const MESSAGE_HEADER_SIZE: usize = 2;
const MESSAGE_TRAILER_SIZE: usize = 3;
const MESSAGE_LENGTH_MIN: usize = MESSAGE_HEADER_SIZE + MESSAGE_TRAILER_SIZE;
const MESSAGE_LENGTH_MAX: usize = 64;
const MESSAGE_POSITION_LENGTH: usize = 0;
const MESSAGE_POSITION_SEQ: usize = 1;
const MESSAGE_TRAILER_CRC: usize = 3;
const MESSAGE_TRAILER_SYNC: usize = 1;
const MESSAGE_VALUE_SYNC: u8 = 0x7E;
const MESSAGE_DEST: u8 = 0x10;
const MESSAGE_SEQ_MASK: u8 = 0x0F;

fn crc16(buf: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for b in buf {
        let b = *b ^ ((crc & 0xFF) as u8);
        let b = b ^ (b << 4);
        let b16 = b as u16;
        crc = (b16 << 8 | crc >> 8) ^ (b16 >> 4) ^ (b16 << 3);
    }
    crc
}

pub trait Config {
    type TransportOutput: TransportOutput;
    type Context<'c>;
    fn dispatch<'c>(
        cmd: u16,
        frame: &mut &[u8],
        context: &mut Self::Context<'c>,
    ) -> Result<(), ReadError>;
}

/// Protocol transport implementation
pub struct Transport<C: Config + 'static> {
    is_synchronized: AtomicBool,
    next_sequence: AtomicU8,
    output: C::TransportOutput,
}

impl<C: Config> Transport<C> {
    #[doc(hidden)]
    pub const fn new(_config: &'static C, output: C::TransportOutput) -> Self {
        Self {
            is_synchronized: AtomicBool::new(true),
            next_sequence: AtomicU8::new(MESSAGE_DEST),
            output,
        }
    }

    /// Decodes messages from an `InputBuffer`
    pub fn receive<'c>(&self, input: &mut impl InputBuffer, mut context: C::Context<'c>) {
        // Drive state machine forward until we either have no
        // input or know we don't have enough input.
        let mut data = input.data();
        while !data.is_empty() {
            if !self.is_synchronized.load(Ordering::SeqCst) {
                // Look for a sync byte
                if let Some(n) = data.iter().position(|b| *b == MESSAGE_VALUE_SYNC) {
                    data = &data[n + 1..];
                    self.is_synchronized.store(true, Ordering::SeqCst);
                    self.encode_acknak();
                } else {
                    data = &[];
                }
            } else {
                if data[0] == MESSAGE_VALUE_SYNC {
                    data = &data[1..];
                    continue;
                }

                if data.len() < MESSAGE_LENGTH_MIN {
                    break;
                }

                let len = data[MESSAGE_POSITION_LENGTH] as usize;
                if !(MESSAGE_LENGTH_MIN..=MESSAGE_LENGTH_MAX).contains(&len) {
                    self.is_synchronized.store(false, Ordering::SeqCst);
                    continue;
                }

                let seq = data[MESSAGE_POSITION_SEQ];
                if seq & !MESSAGE_SEQ_MASK != MESSAGE_DEST {
                    self.is_synchronized.store(false, Ordering::SeqCst);
                    continue;
                }
                if data.len() < len {
                    break;
                }
                if data[len - MESSAGE_TRAILER_SYNC] != MESSAGE_VALUE_SYNC {
                    self.is_synchronized.store(false, Ordering::SeqCst);
                    continue;
                }

                let frame_crc = ((data[len - MESSAGE_TRAILER_CRC] as u16) << 8)
                    | (data[len - MESSAGE_TRAILER_CRC + 1] as u16);
                let actual_crc = crc16(&data[0..len - MESSAGE_TRAILER_SIZE]);
                if frame_crc != actual_crc {
                    self.is_synchronized.store(false, Ordering::SeqCst);
                    continue;
                }

                let frame = &data[MESSAGE_HEADER_SIZE..len - MESSAGE_TRAILER_SIZE];
                data = &data[len..];
                if seq == self.next_sequence.load(Ordering::SeqCst) {
                    self.next_sequence.store(
                        ((seq + 1) & MESSAGE_SEQ_MASK) | MESSAGE_DEST,
                        Ordering::SeqCst,
                    );
                    let _ = self.parse_frame(frame, &mut context);
                }
                self.encode_acknak();
            }
        }
        // Remove consumed bytes from front
        let consumed = input.available() - data.len();
        if consumed > 0 {
            input.pop(consumed);
        }
    }

    fn parse_frame<'c>(
        &self,
        mut frame: &[u8],
        context: &mut C::Context<'c>,
    ) -> Result<(), ReadError> {
        while !frame.is_empty() {
            let cmd = <u16 as Readable>::read(&mut frame)?;
            C::dispatch(cmd, &mut frame, context)?;
        }
        Ok(())
    }

    // Fast path for ACK/NAK
    fn encode_acknak(&self) {
        self.output.output(|output| {
            let ns = self.next_sequence.load(Ordering::SeqCst);
            let crc = crc16(&[5, ns]);
            output.output(&[
                5,
                ns,
                ((crc & 0xFF00) >> 8) as u8,
                (crc & 0xFF) as u8,
                MESSAGE_VALUE_SYNC,
            ]);
        });
    }

    #[doc(hidden)]
    pub fn encode_frame(
        &self,
        f: impl FnOnce(&mut <<C as Config>::TransportOutput as TransportOutput>::Output),
    ) {
        self.output.output(|output| {
            let cursor = output.cur_position();
            output.output(&[0, self.next_sequence.load(Ordering::SeqCst)]); // Output header
            f(output); // Output actual frame contents
            {
                let changed = output.data_since(cursor).len();
                output.update(cursor, (changed + MESSAGE_TRAILER_SIZE) as u8);
            }
            let crc = crc16(output.data_since(cursor));
            output.output(&[
                ((crc & 0xFF00) >> 8) as u8,
                (crc & 0xFF) as u8,
                MESSAGE_VALUE_SYNC,
            ]);
        })
    }
}
