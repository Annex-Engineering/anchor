#![no_std]
#![no_main]
#![allow(dead_code)]
#![allow(unused_mut)]

mod clock;
mod commands;
mod usb;

use anchor::*;
pub use esp32c3_hal as hal;
use esp_backtrace as _;
use hal::{clock::ClockControl, peripherals::Peripherals, prelude::*, timer::TimerGroup, Rtc};
use riscv_rt::entry;

pub struct State {
    clock: clock::Clock,
    config_crc: Option<u32>,
}

impl State {
    fn poll(&mut self) {}
}
pub struct Esp32c3Device {
    usb: usb::Esp32c3UsbDevice,
    receive_buffer: FifoBuffer<{ usb::USB_MAX_PACKET_SIZE * 2 }>,
    state: State,
}

impl Esp32c3Device {
    fn new() -> Esp32c3Device {
        let peripherals = Peripherals::take();
        let system = peripherals.SYSTEM.split();
        let clocks = ClockControl::boot_defaults(system.clock_control).freeze();

        let mut rtc = Rtc::new(peripherals.RTC_CNTL);
        let timer_group0 = TimerGroup::new(peripherals.TIMG0, &clocks);
        let mut timer0 = timer_group0.timer0;
        let mut wdt0 = timer_group0.wdt;
        let timer_group1 = TimerGroup::new(peripherals.TIMG1, &clocks);
        let mut wdt1 = timer_group1.wdt;

        // Disable watchdog timers
        rtc.swd.disable();
        rtc.rwdt.disable();
        wdt0.disable();
        wdt1.disable();
        let mut st = State {
            clock: clock::Clock::new(timer0),
            config_crc: None,
        };
        st.clock.start_timer();

        Esp32c3Device {
            usb: usb::Esp32c3UsbDevice::new(peripherals.USB_DEVICE),
            receive_buffer: FifoBuffer::<{ usb::USB_MAX_PACKET_SIZE * 2 }>::new(),
            state: st,
        }
    }

    fn run_forever(mut self) -> ! {
        loop {
            self.state.poll();

            self.usb.read_into(&mut self.receive_buffer);
            let recv_data = self.receive_buffer.data();
            if !recv_data.is_empty() {
                let mut wrap = SliceInputBuffer::new(recv_data);
                KLIPPER_TRANSPORT.receive(&mut wrap, &mut self.state);
                let consumed = recv_data.len() - wrap.available();
                if consumed > 0 {
                    self.receive_buffer.pop(consumed);
                }
            }

            critical_section::with(|cs| {
                let mut txbuf = usb::USB_TX_BUFFER.borrow(cs).borrow_mut();
                self.usb.write_from(&mut txbuf);
            });
        }
    }
}

#[entry]
fn main() -> ! {
    Esp32c3Device::new().run_forever();
}

klipper_config_generate!(
    transport = crate::usb::TRANSPORT_OUTPUT: crate::usb::BufferTransportOutput,
    context = &'ctx mut crate::State,
);

#[klipper_constant]
const MCU: &str = "esp32c3_custom";

#[klipper_constant]
const STATS_SUMSQ_BASE: u32 = 256;
