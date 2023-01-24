# Anchor testjig

This project contains a small testjig that was used to develop Anchor. We've
included it as an example to show off how one could make a very simple PTY based
Klipper MCU.

To run, simply execute:

```
% KLIPPER_PATH=~/path/to/klipper cargo run --features skipped_command
```

Replace `~/path/to/klipper` with the correct path. One may also leave out the
`--features skipped_command` part. This demonstrates the use of compile-time
enabling and disabling of commands based on feature flags.
