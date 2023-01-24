# Anchor rp2040 demo

This project contains a small example of integrating Anchor in an rp2040 based
project, in this example for the Pico board.

To compile the project, you need a Rust toolchain and the `elf2uf2-rs` utility.
This can be installed by running `cargo install elf2uf2-rs`.

To test the project, mount a Pico in BOOTSEL mode and run:
```
% cargo build --release --target thumbv6m-none-eabi
% sudo elf2uf2-rs -d target/thumbv6m-none-eabi/release/rp2040_demo
```

This will flash the pico and run the project. The device should reboot and
present a serial device, usually found at
`/dev/serial/by-id/usb-Anchor_rp2040_demo_static-if00`. To have Klippy
communicate with the device, a very basic config like the following can be used:

```ini
[mcu]
serial: /dev/serial/by-id/usb-Anchor_rp2040_demo_static-if00

[printer]
kinematics: none
max_velocity: 100
max_accel: 100
```

Simply launch `klippy` from the `Klipper` directory, pointing at the config file:
```
% python klippy/klippy.py /path/to/config.cfg
```
