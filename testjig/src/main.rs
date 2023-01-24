use anchor::*;
use lazy_static::lazy_static;
use std::{
    env,
    io::Write,
    os::unix::io::RawFd,
    path::PathBuf,
    process::{self, Command},
    sync::Mutex,
};
use tempfile::TempDir;

klipper_config_generate!(transport = crate::TRANSPORT_OUTPUT: crate::BufferTransportOutput);

struct KlipperInstance {
    _temp_dir: TempDir,
    child: process::Child,
}

impl KlipperInstance {
    fn new(cfg: impl AsRef<str>) -> Self {
        let klipper_path = env::var("KLIPPER_PATH")
            .expect("set `KLIPPER_PATH` environment variable to klipper path");
        let klipper_path = std::fs::canonicalize(klipper_path).expect("Klipper not found");

        let temp_dir = TempDir::new().expect("Could not create work directory");
        let cfg_filename = temp_dir.path().join("klippy.cfg");
        {
            let mut cfg_file =
                std::fs::File::create(&cfg_filename).expect("Could not open config file");
            cfg_file
                .write_all(cfg.as_ref().as_bytes())
                .expect("Could not write config file");
        }

        let child = Command::new("python3")
            .current_dir(klipper_path)
            .arg("klippy/klippy.py")
            .arg(cfg_filename)
            .spawn()
            .expect("Could not launch klippy");

        KlipperInstance {
            _temp_dir: temp_dir,
            child,
        }
    }
}

impl Drop for KlipperInstance {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

struct SerialEmulator {
    master: RawFd,
    slave: RawFd,
}

impl SerialEmulator {
    fn new() -> Self {
        use nix::sys::termios::*;

        let termios: Termios = unsafe { std::mem::zeroed() };

        let ptys = nix::pty::openpty(None, &Some(termios)).expect("Could not allocate pty");

        SerialEmulator {
            master: ptys.master,
            slave: ptys.slave,
        }
    }

    fn ttyname(&self) -> PathBuf {
        nix::unistd::ttyname(self.slave).expect("Could not get TTY name")
    }

    fn master(&self) -> RawFd {
        self.master
    }
}

impl Drop for SerialEmulator {
    fn drop(&mut self) {
        let _ = nix::unistd::close(self.master);
        let _ = nix::unistd::close(self.slave);
    }
}

static TRANSPORT_OUTPUT_MUTEX: Mutex<Option<RawFd>> = Mutex::new(None);

#[derive(Debug, Default)]
struct BufferTransportOutput;

impl TransportOutput for BufferTransportOutput {
    type Output = ScratchOutput;
    fn output(&self, f: impl FnOnce(&mut Self::Output)) {
        if let Some(fd) = TRANSPORT_OUTPUT_MUTEX.lock().unwrap().as_ref() {
            let mut scratch = ScratchOutput::new();
            f(&mut scratch);
            let result = scratch.result();
            if !result.is_empty() {
                let n = nix::unistd::write(*fd, result).expect("Could not write");
                if n != result.len() {
                    panic!("Could not write full message");
                }
            }
        }
    }
}

pub(crate) const TRANSPORT_OUTPUT: BufferTransportOutput = BufferTransportOutput;

fn main() {
    let serial = SerialEmulator::new();
    *TRANSPORT_OUTPUT_MUTEX.lock().unwrap() = Some(serial.master());

    for i in 0..=(Pins::max_variant() as u8) {
        let p: Result<Pins, _> = i.try_into();
        match p {
            Err(_) => panic!("Can't map pin {i}"),
            Ok(p) => {
                if i != p.into() {
                    panic!("Can't reverse map pin {i}")
                }
            }
        }
    }

    let _instance = KlipperInstance::new(format!(
        r#"
            [mcu]
            serial: {}

            [printer]
            kinematics: none
            max_velocity: 100
            max_accel: 100
        "#,
        serial.ttyname().display()
    ));

    let mut recv = [0u8; 128];
    let mut rcvbuf: Vec<u8> = Vec::new();
    loop {
        match nix::unistd::read(serial.master(), &mut recv) {
            Err(nix::errno::Errno::EWOULDBLOCK) => {}
            Err(e) => panic!("read failed: {e})"),
            Ok(n) => {
                rcvbuf.extend(&recv[..n]);
                KLIPPER_TRANSPORT.receive(&mut rcvbuf, ());
            }
        };
        if cur_clock() > 10 * CLOCK_FREQ {
            klipper_output!("This the %uth test! %*s?", Pins::PB8.into(), "You alright?");
            klipper_shutdown!("This is a test!", cur_clock());
        }
    }
}

fn cur_clock() -> u32 {
    use std::time::Instant;
    lazy_static! {
        static ref BEGIN: Instant = Instant::now();
    }
    let c = (BEGIN.elapsed().as_secs_f64() * (CLOCK_FREQ as f64)).floor() as u64;
    (c & 0xFFFFFFFF) as u32
}

#[klipper_command]
fn get_uptime(_context: &()) {
    klipper_reply!(uptime, high: u32 = 2, clock: u32 = cur_clock());
}

#[klipper_command]
fn get_clock() {
    klipper_reply!(clock, clock: u32 = cur_clock());
}

#[klipper_command]
fn emergency_stop() {}

lazy_static! {
    static ref CONFIG_CRC: Mutex<Option<u32>> = Mutex::new(None);
}

#[klipper_command]
fn get_config() {
    let crc = CONFIG_CRC.lock().unwrap();
    klipper_reply!(
        config,
        is_config: bool = crc.is_some(),
        crc: u32 = crc.unwrap_or(0),
        is_shutdown: bool = false,
        move_count: u16 = 0
    );
}

#[klipper_command]
fn config_reset() {
    *CONFIG_CRC.lock().unwrap() = None;
}

#[klipper_command]
fn finalize_config(crc: u32) {
    *CONFIG_CRC.lock().unwrap() = Some(crc);
}

#[klipper_command]
fn allocate_oids(count: u8) {
    let _ = count;
}

#[klipper_command]
fn test_array(buf: &[u8], offset: u16) {
    let _ = buf;
    let _ = offset;
}

#[klipper_command]
#[cfg(feature = "skipped_command")]
fn must_skip() {
    klipper_output!("Output in a skipped command!");
    klipper_reply!(reply_in_skipped);
}

#[klipper_constant]
const CLOCK_FREQ: u32 = 100_000_000;

#[klipper_constant]
const MCU: &str = "anchor_jig";

#[klipper_constant]
const STATS_SUMSQ_BASE: u32 = 256;

#[cfg(feature = "skipped_command")]
#[klipper_constant]
const TEST: &str = "skipped most of the time";

klipper_enumeration! {
    #[derive(Debug)]
    #[klipper_enumeration(name = "spi_bus", rename_all = "snake_case")]
    #[allow(dead_code)]
    enum SpiBus {
        #[klipper_enumeration(rename = "spi0a")]
        Spi0A,
        #[klipper_enumeration(rename = "spi0b")]
        Spi0B,
        Spi0C,
        Spi0D,
        Spi1A,
        Spi1B,
        #[cfg(feature="skipped_command")]
        Spi1C,
    }
}

klipper_enumeration! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq)]
    #[klipper_enumeration(name = "pin", rename_all = "UPPERCASE")]
    enum Pins {
        Range(PA, 0, 16),
        Range(PB, 0, 16),
        AdcTemperature,
    }
}

mod test_embed {
    use anchor::*;
    #[klipper_command]
    pub fn woot() {}
}

mod test;

#[cfg(feature = "skipped_command")]
mod test_skipped {
    use anchor::*;
    #[klipper_command]
    pub fn skipped_command_in_module() {}
}

#[klipper_command]
fn wee() {}
