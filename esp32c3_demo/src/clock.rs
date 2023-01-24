use crate::hal::{
    peripherals::TIMG0,
    timer::{Timer, Timer0},
};
use anchor::*;
use esp32c3_hal::timer::Instance;

pub struct Clock {
    pub timer: Timer<Timer0<TIMG0>>,
}

impl Clock {
    pub fn new(timer: Timer<Timer0<TIMG0>>) -> Clock {
        Clock { timer }
    }

    pub fn start_timer(&mut self) {
        self.timer.reset_counter();
        self.timer.set_counter_active(true);
    }

    pub fn low(&self) -> InstantShort {
        // prevent stale values due to hal/platform bug by doing a dummy double read
        _ = self.timer.now();
        InstantShort(self.timer.now() as u32)
    }

    pub fn full(&self) -> InstantFull {
        _ = self.timer.now();
        InstantFull(self.timer.now() & 0x003F_FFFF_FFFF_FFFF)
    }
}

#[derive(Copy, Clone)]
pub struct InstantShort(u32);

impl InstantShort {
    pub fn new(t: u32) -> InstantShort {
        InstantShort(t)
    }

    pub fn after(&self, other: impl AsRef<Self>) -> bool {
        other.as_ref().0.wrapping_sub(self.0) & 0x8000_0000 != 0
    }
}

impl core::ops::AddAssign<u32> for InstantShort {
    fn add_assign(&mut self, rhs: u32) {
        self.0 = self.0.wrapping_add(rhs);
    }
}

impl core::ops::Add<u32> for InstantShort {
    type Output = Self;
    fn add(self, rhs: u32) -> Self::Output {
        InstantShort(self.0.wrapping_add(rhs))
    }
}

impl core::convert::AsRef<InstantShort> for InstantShort {
    fn as_ref(&self) -> &InstantShort {
        self
    }
}

impl From<InstantShort> for u32 {
    fn from(t: InstantShort) -> Self {
        t.0
    }
}

#[derive(Copy, Clone)]
pub struct InstantFull(u64);

impl From<InstantFull> for u64 {
    fn from(t: InstantFull) -> Self {
        t.0
    }
}

#[klipper_constant]
const CLOCK_FREQ: u32 = 40_000_000;

#[klipper_command]
pub fn get_uptime(context: &mut crate::State) {
    let c = context.clock.full().0;
    klipper_reply!(
        uptime,
        high: u32 = (c >> 32) as u32,
        clock: u32 = (c & 0xFFFF_FFFF) as u32
    );
}

#[klipper_command]
pub fn get_clock(context: &mut crate::State) {
    klipper_reply!(clock, clock: u32 = context.clock.low().0);
}
