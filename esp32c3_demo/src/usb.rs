use crate::hal::{peripherals::USB_DEVICE, prelude::*, UsbSerialJtag};
use anchor::*;
use core::cell::RefCell;
use critical_section::Mutex;

pub const USB_MAX_PACKET_SIZE: usize = 64;
static USB_SERIAL: Mutex<RefCell<Option<UsbSerialJtag<USB_DEVICE>>>> =
    Mutex::new(RefCell::new(None));
pub struct Esp32c3UsbDevice {
    need_flush: bool,
}

impl Esp32c3UsbDevice {
    pub fn new(usb_device: crate::hal::peripherals::USB_DEVICE) -> Esp32c3UsbDevice {
        let mut usb_serial = UsbSerialJtag::new(usb_device);
        critical_section::with(|cs| USB_SERIAL.borrow_ref_mut(cs).replace(usb_serial));

        Esp32c3UsbDevice { need_flush: false }
    }

    pub fn read_into<const BUF_SIZE: usize>(&mut self, buffer: &mut FifoBuffer<BUF_SIZE>) {
        critical_section::with(|cs| {
            let mut usb_serial = USB_SERIAL.borrow_ref_mut(cs);
            let usb_serial_ref = usb_serial.as_mut().unwrap();

            while let nb::Result::Ok(c) = usb_serial_ref.read_byte() {
                buffer.extend(&[c])
            }
        });
    }

    pub fn write_from<const BUF_SIZE: usize>(&mut self, buffer: &mut FifoBuffer<BUF_SIZE>) {
        critical_section::with(|cs| {
            // Fast path: nothing to do
            if !buffer.is_empty() {
                let data = buffer.data();

                let mut usb_serial = USB_SERIAL.borrow_ref_mut(cs);
                let mut usb_serial = usb_serial.as_mut().unwrap();

                let mut consumed = 0;
                for i in 0..data.len() {
                    match usb_serial.write_byte_nb(data[i]) {
                        Ok(_) => consumed += 1,
                        Err(_) => break,
                    }
                }
                if consumed > 0 {
                    buffer.pop(consumed);
                    self.need_flush = true;
                }
            }

            if self.need_flush {
                let mut usb_serial = USB_SERIAL.borrow_ref_mut(cs);
                let mut usb_serial = usb_serial.as_mut().unwrap();
                let _ = usb_serial.flush_tx_nb().ok();
                self.need_flush = false;
            }
        });
    }
}

pub static USB_TX_BUFFER: Mutex<RefCell<FifoBuffer<{ USB_MAX_PACKET_SIZE * 2 }>>> =
    Mutex::new(RefCell::new(FifoBuffer::new()));
pub(crate) struct BufferTransportOutput;

impl TransportOutput for BufferTransportOutput {
    type Output = ScratchOutput;
    fn output(&self, f: impl FnOnce(&mut Self::Output)) {
        let mut scratch = ScratchOutput::new();
        f(&mut scratch);
        let output = scratch.result();
        critical_section::with(|cs| USB_TX_BUFFER.borrow(cs).borrow_mut().extend(output));
    }
}

pub(crate) const TRANSPORT_OUTPUT: BufferTransportOutput = BufferTransportOutput;
