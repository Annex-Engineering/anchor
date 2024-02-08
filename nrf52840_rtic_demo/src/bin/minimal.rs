#![no_main]
#![no_std]
#![feature(type_alias_impl_trait)]

use anchor::klipper_config_generate;

#[rtic::app(
    device = nrf52840_hal::pac,
    dispatchers = [SWI0_EGU0],
)]
mod app {

    use anchor::*;
    use core::cell::RefCell;
    use core::mem::MaybeUninit;
    use embassy_nrf::usb::{vbus_detect::HardwareVbusDetect, Driver as UsbDriver};
    use embassy_sync::{
        blocking_mutex::{raw::CriticalSectionRawMutex, CriticalSectionMutex},
        signal::Signal,
    };
    use embassy_usb::{
        class::cdc_acm::{self, CdcAcmClass},
        driver::EndpointError,
        Builder as UsbBuilder, Config as UsbConfig, UsbDevice,
    };
    use rtic_monotonics::{nrf::timer::Timer4 as Timer, Monotonic};
    use test_app as _;

    embassy_nrf::bind_interrupts!(struct Irqs {
        USBD => embassy_nrf::usb::InterruptHandler<embassy_nrf::peripherals::USBD>;
        POWER_CLOCK => embassy_nrf::usb::vbus_detect::InterruptHandler;
    });

    #[derive(defmt::Format)]
    pub struct AppState {
        config_crc: Option<u32>,
    }

    // Shared resources go here
    #[shared]
    struct Shared {
        app_state: AppState,
    }

    type HalUsbDriver =
        embassy_nrf::usb::Driver<'static, embassy_nrf::peripherals::USBD, HardwareVbusDetect>;

    // Local resources go here
    #[local]
    struct Local {
        usb: UsbDevice<'static, HalUsbDriver>,
        cdc_sender: cdc_acm::Sender<'static, HalUsbDriver>,
        cdc_receiver: cdc_acm::Receiver<'static, HalUsbDriver>,
        cdc_control: cdc_acm::ControlChanged<'static>,
    }

    struct UsbData {
        device_descriptor: [u8; 256],
        config_descriptor: [u8; 256],
        bos_descriptor: [u8; 256],
        msos_descriptor: [u8; 256],
        control_buf: [u8; 64],
    }

    impl UsbData {
        const fn new() -> Self {
            UsbData {
                device_descriptor: [0u8; 256],
                config_descriptor: [0u8; 256],
                bos_descriptor: [0u8; 256],
                msos_descriptor: [0u8; 256],
                control_buf: [0u8; 64],
            }
        }
    }

    #[init(local = [usb_data: UsbData = UsbData::new(), usb_acm_state: MaybeUninit<cdc_acm::State<'static>> = MaybeUninit::uninit()])]
    fn init(cx: init::Context) -> (Shared, Local) {
        defmt::info!("init");

        let p = embassy_nrf::init(Default::default());
        let clock: nrf52840_hal::pac::CLOCK = unsafe { core::mem::transmute(()) };

        clock.tasks_hfclkstart.write(|w| unsafe { w.bits(1) });
        while clock.events_hfclkstarted.read().bits() != 1 {}

        let driver = UsbDriver::new(p.USBD, Irqs, HardwareVbusDetect::new(Irqs));
        let mut config = UsbConfig::new(0xc0de, 0xcafe);
        config.manufacturer = Some("Annex Engineering");
        config.product = Some("Anchor RTIC jig");
        config.serial_number = Some("1234");
        config.max_power = 100;
        config.max_packet_size_0 = 64;
        config.device_class = 0xEF;
        config.device_sub_class = 0x02;
        config.device_protocol = 0x01;
        config.composite_with_iads = true;

        let mut builder = UsbBuilder::new(
            driver,
            config,
            &mut cx.local.usb_data.device_descriptor,
            &mut cx.local.usb_data.config_descriptor,
            &mut cx.local.usb_data.bos_descriptor,
            &mut cx.local.usb_data.msos_descriptor,
            &mut cx.local.usb_data.control_buf,
        );

        let class = CdcAcmClass::new(
            &mut builder,
            cx.local.usb_acm_state.write(Default::default()),
            64,
        );

        let usb = builder.build();
        usb_pump::spawn().ok();
        usb_task_receive::spawn().ok();
        usb_task_send::spawn().ok();
        usb_task_control::spawn().ok();

        let (cdc_sender, cdc_receiver, cdc_control) = class.split_with_control();

        let token = rtic_monotonics::create_nrf_timer4_monotonic_token!();
        let timer4: nrf52840_hal::pac::TIMER4 = unsafe { core::mem::transmute(()) };
        Timer::start(timer4, token);

        (
            Shared {
                // Initialization of shared resources go here
                app_state: AppState { config_crc: None },
            },
            Local {
                // Initialization of local resources go here
                usb,
                cdc_sender,
                cdc_receiver,
                cdc_control,
            },
        )
    }

    #[idle]
    fn idle(_cx: idle::Context) -> ! {
        loop {
            rtic::export::wfi()
        }
    }

    #[task(priority = 1, local = [usb])]
    async fn usb_pump(cx: usb_pump::Context) {
        cx.local.usb.run().await;
    }

    #[task(priority = 1, local = [cdc_receiver], shared = [app_state])]
    async fn usb_task_receive(mut cx: usb_task_receive::Context) {
        let receiver = cx.local.cdc_receiver;
        loop {
            defmt::info!("USB waiting for connection");
            receiver.wait_connection().await;
            loop {
                let mut rcv_buf = FifoBuffer::<128>::new();
                match receiver.read_packet(rcv_buf.receive_buffer()).await {
                    Ok(n) => {
                        defmt::trace!("Klipper protocol RECEIVE, {} bytes", n);
                        rcv_buf.advance(n);
                        if !rcv_buf.is_empty() {
                            let mut wrap = SliceInputBuffer::new(rcv_buf.data());
                            cx.shared.app_state.lock(|app_state| {
                                crate::KLIPPER_TRANSPORT.receive(&mut wrap, app_state);
                            });
                            rcv_buf.pop(rcv_buf.len() - wrap.available());
                        }
                    }
                    Err(_) => {
                        defmt::error!("Lost USB connection");
                        break;
                    }
                }
            }
        }
    }

    #[task(priority = 1, local = [cdc_sender])]
    async fn usb_task_send(cx: usb_task_send::Context) {
        let sender = cx.local.cdc_sender;
        loop {
            sender.wait_connection().await;
            loop {
                let mut tx_buf = FifoBuffer::<128>::new();
                loop {
                    if tx_buf.is_empty() {
                        USB_TX_WAITING.wait().await;
                        USB_TX_BUFFER.lock(|buffer| {
                            let mut buffer = buffer.borrow_mut();
                            let n = buffer.len();
                            tx_buf.extend(buffer.data());
                            buffer.pop(n);
                        })
                    }
                    if tx_buf.is_empty() {
                        continue;
                    }
                    let n = tx_buf.len().clamp(0, sender.max_packet_size() as usize);
                    defmt::trace!("Klipper protocol SEND, {} bytes", n);
                    match sender.write_packet(&tx_buf.data()[..n]).await {
                        Ok(_) => tx_buf.pop(n),
                        Err(EndpointError::BufferOverflow) => {}
                        Err(e) => {
                            defmt::error!("USB send error: {}", e);
                            break;
                        }
                    }
                }
            }
        }
    }

    #[task(priority = 1, local = [cdc_control])]
    async fn usb_task_control(cx: usb_task_control::Context) {
        loop {
            cx.local.cdc_control.control_changed().await;
        }
    }

    #[klipper_constant]
    const CLOCK_FREQ: u32 = 1_000_000;

    #[klipper_constant]
    const MCU: &str = "rtic";
    #[klipper_constant]
    const STATS_SUMSQ_BASE: u32 = 256;
    #[klipper_constant]
    const ADC_MAX: u32 = 4095;

    #[klipper_command]
    pub fn get_uptime(_context: &mut AppState) {
        let c = Timer::now().ticks();
        klipper_reply!(
            uptime,
            high: u32 = (c >> 32) as u32,
            clock: u32 = (c & 0xFFFFFFFF) as u32
        );
    }

    #[klipper_command]
    pub fn get_clock(_context: &mut AppState) {
        klipper_reply!(clock, clock: u32 = Timer::now().ticks() as u32);
    }

    #[klipper_command]
    pub fn emergency_stop() {}

    #[klipper_command]
    pub fn get_config(context: &mut AppState) {
        let crc = context.config_crc;
        klipper_reply!(
            config,
            is_config: bool = crc.is_some(),
            crc: u32 = crc.unwrap_or(0),
            is_shutdown: bool = false,
            move_count: u16 = 0
        );
    }

    #[klipper_command]
    pub fn config_reset(context: &mut AppState) {
        context.config_crc = None;
    }

    #[klipper_command]
    pub fn finalize_config(context: &mut AppState, crc: u32) {
        context.config_crc = Some(crc);
    }

    #[klipper_command]
    pub fn allocate_oids(_count: u8) {}

    #[klipper_command]
    pub fn debug_nop() {}

    static USB_TX_BUFFER: CriticalSectionMutex<RefCell<FifoBuffer<128>>> =
        CriticalSectionMutex::new(RefCell::new(FifoBuffer::new()));
    static USB_TX_WAITING: Signal<CriticalSectionRawMutex, ()> = Signal::new();
    pub struct BufferTransportOutput;
    pub const TRANSPORT_OUTPUT: BufferTransportOutput = BufferTransportOutput;

    impl TransportOutput for BufferTransportOutput {
        type Output = ScratchOutput;
        fn output(&self, f: impl FnOnce(&mut Self::Output)) {
            let mut scratch = ScratchOutput::new();
            f(&mut scratch);
            let output = scratch.result();
            USB_TX_BUFFER.lock(|buffer| buffer.borrow_mut().extend(output));
            USB_TX_WAITING.signal(());
        }
    }
}

klipper_config_generate!(
  transport = crate::app::TRANSPORT_OUTPUT: crate::app::BufferTransportOutput,
  context = &'ctx mut crate::app::AppState,
);
