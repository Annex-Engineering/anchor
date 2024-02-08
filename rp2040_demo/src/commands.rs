use crate::State;
use anchor::*;

#[klipper_command]
pub fn debug_nop() {}

#[klipper_command]
pub fn emergency_stop() {}

#[klipper_command]
pub fn get_config(context: &State) {
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
pub fn config_reset(context: &mut State) {
    context.config_crc = None;
}

#[klipper_command]
pub fn finalize_config(context: &mut State, crc: u32) {
    context.config_crc = Some(crc);
}

#[klipper_command]
pub fn allocate_oids(_count: u8) {}
