#![no_std]
#![no_main]

mod clock;
mod commands;
mod usb;

use rp_pico as bsp;

use panic_halt as _;

use bsp::{
    entry,
    hal::{clocks::init_clocks_and_plls, pac, usb::UsbBus, watchdog::Watchdog},
};
use cortex_m::interrupt::free;
use usb_device::{class_prelude::UsbBusAllocator, prelude::*};
use usbd_serial::{CdcAcmClass, USB_CLASS_CDC};

use anchor::*;
use usb::*;

pub struct State {
    clock: clock::Clock,
    config_crc: Option<u32>,
}

impl State {
    fn poll(&mut self) {}
}

#[entry]
fn main() -> ! {
    let mut pac = pac::Peripherals::take().unwrap();
    let mut watchdog = Watchdog::new(pac.WATCHDOG);

    // External high-speed crystal on the pico board is 12Mhz
    let external_xtal_freq_hz = bsp::XOSC_CRYSTAL_FREQ;
    let clocks = init_clocks_and_plls(
        external_xtal_freq_hz,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    let usb_allocator = UsbBusAllocator::new(UsbBus::new(
        pac.USBCTRL_REGS,
        pac.USBCTRL_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));

    let mut serial = CdcAcmClass::new(&usb_allocator, 64);
    let mut bus = UsbDeviceBuilder::new(&usb_allocator, UsbVidPid(0x1d50, 0x614e))
        .composite_with_iads()
        .manufacturer("Anchor")
        .product("rp2040_demo")
        .serial_number("static")
        .device_class(USB_CLASS_CDC)
        .build();

    let mut read_buffer = FifoBuffer::<128>::new();
    let mut packet_writer = UsbPacketWriter::default();

    let mut state = State {
        clock: clock::Clock::new(pac.TIMER),
        config_crc: None,
    };

    loop {
        state.poll();

        // Read side
        bus.poll(&mut [&mut serial]);
        while let Ok(n) = serial.read_packet(read_buffer.receive_buffer()) {
            read_buffer.advance(n);
        }
        if !read_buffer.is_empty() {
            let mut wrap = SliceInputBuffer::new(read_buffer.data());
            KLIPPER_TRANSPORT.receive(&mut wrap, &mut state);
            read_buffer.pop(read_buffer.len() - wrap.available());
        }

        // Write side
        free(|cs| {
            let mut txbuf = USB_TX_BUFFER.borrow(cs).borrow_mut();
            packet_writer.write_packets(&mut serial, &mut txbuf);
        });
        bus.poll(&mut [&mut serial]);
    }
}

klipper_config_generate!(
    transport = crate::usb::TRANSPORT_OUTPUT: crate::usb::BufferTransportOutput,
    context = &'ctx mut crate::State,
);

#[klipper_constant]
const MCU: &str = "rp2040_custom";

#[klipper_constant]
const STATS_SUMSQ_BASE: u32 = 256;
