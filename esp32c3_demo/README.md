# Anchor esp32c3 demo

This project contains a small example of integrating Anchor in an esp32c3
based project. The example was originally built on a Seeed Studio XIAO
ESP32C3 and also tested on a WeAct Studio ESP32-C3FH4. Using a different
board may require a different setup.

Please note that there are multiple instances where `timer.now()` is called
twice, this is intentional and should be considered required. There is a bug
in the esp32c3 hal that causes unexpected behavior.

you will need to install the following:
the riscv target with the command `rustup target add riscv32imc-unknown-none-elf`
espflash with `cargo install cargo-espflash`


To test the project in windows run the following where X is the com port the board is attached to:
```
% cargo espflash --release  <COMX>
```

To test the project in linux run the following where X is the tty the board is attached to (for example ttyACM0):
```
% cargo espflash --release  <ttyXXX>
```

This will flash the device and run the project. The device should reboot and
present a serial device, usually found at
`/dev/serial/by-id/XXXX`. To have Klippy
communicate with the device, a very basic config like the following can be used:

```ini
[mcu]
serial: /dev/serial/by-id/<XXXX>

[printer]
kinematics: none
max_velocity: 100
max_accel: 100
```

(For the following you might want to use the klippyenv version of python which has dependencies setup, for this launch python from `~/klippy-env/bin/python`)
Simply launch `klippy` from the `Klipper` directory, pointing at the config file:
```
% python klippy/klippy.py /path/to/config.cfg
```
