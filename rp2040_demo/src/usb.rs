use anchor::*;
use core::cell::RefCell;
use cortex_m::interrupt::{free, Mutex};
use usb_device::UsbError;
use usbd_serial::CdcAcmClass;

pub static USB_TX_BUFFER: Mutex<RefCell<FifoBuffer<128>>> =
    Mutex::new(RefCell::new(FifoBuffer::new()));
pub(crate) struct BufferTransportOutput;

impl TransportOutput for BufferTransportOutput {
    type Output = ScratchOutput;
    fn output(&self, f: impl FnOnce(&mut Self::Output)) {
        let mut scratch = ScratchOutput::new();
        f(&mut scratch);
        let output = scratch.result();
        free(|cs| USB_TX_BUFFER.borrow(cs).borrow_mut().extend(output));
    }
}

pub(crate) const TRANSPORT_OUTPUT: BufferTransportOutput = BufferTransportOutput;

#[derive(Default)]
pub struct UsbPacketWriter {
    full_count: u8,
}

impl UsbPacketWriter {
    pub fn write_packets<const BUF_SIZE: usize, A: usb_device::class_prelude::UsbBus>(
        &mut self,
        serial: &mut CdcAcmClass<A>,
        buffer: &mut FifoBuffer<BUF_SIZE>,
    ) {
        if buffer.is_empty() && self.full_count == 0 {
            // Fast path: nothing to do
            return;
        }
        let max_packet_size = serial.max_packet_size();
        let data = buffer.data();
        let len = data.len().clamp(0, max_packet_size as usize) as u16;
        let data = &data[..(len as usize)];

        let (consumed, write) = if len == max_packet_size && self.full_count > 10 {
            // Write one byte less
            (len - 1, &data[..(len - 1) as usize])
        } else if len == 0 {
            // Write zero length packet
            (0u16, &[] as &[u8])
        } else {
            // Normal write
            (len, data)
        };

        match serial.write_packet(write) {
            Ok(0) => {
                self.full_count = 0;
            }
            Ok(n) => {
                if (n as u16) < max_packet_size {
                    self.full_count = 0;
                }
                buffer.pop(n)
            }
            Err(UsbError::WouldBlock) => {} // Don't consume from the input buffer
            Err(_) => buffer.pop(consumed as usize), // Ignore errors but consume the data
        }
    }
}
