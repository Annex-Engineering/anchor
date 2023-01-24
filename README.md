# Anchor

Anchor is an implementation of the Klipper protocol.

You can use Anchor to create custom Klipper MCUs. It's written in Rust and
provides only the protocol implementation, giving you full control over how you
want to tie the protocol handling in to your program. You can even implement a
Klipper MCU that runs over a PTY, like the Linux `klipper_mcu` program.

This repo contains the following folders:

  * `anchor`  
    The runtime support library for Anchor. It includes all the functionality
    that is used during runtime.

  * `anchor_codegen`  
    The Klipper protocol requires exchanging an initial data dictionary, and
    command IDs need to be hooked up. `anchor_codegen` creates this data
    dictionary, message handlers, serializers, etc. and generates a Rust module
    that will be included by the `klipper_generate_config` macro in your
    project.

  * `rp2040_demo`  
    A simple demo showing how one could integrate Anchor in an rp2040 project,
    communicating over USB.

  * `esp32c3_demo`  
    A simple demo showing how one could integrate Anchor in an esp32c3 project,
    communicating over USB.

  * `testjig`  
    A development tool and example of how to use Anchor for implementing a very
    simple PTY based MCU that can talk to Klipper.

  * `anchor_macro`  
    Implements the `proc_macro`s needed by `anchor`. You shouldn't have to mess
    with this.

Anchor powers the [Beacon3D Surface Scanner](https://beacon3d.com/).

## Documentation

Documentation can be found [here](https://anchor.annex.engineering).

## Licensing

Anchor is licensed under the MIT license, which means you can do pretty much
whatever you want with it. Please see [LICENSE.txt](LICENSE.txt) for more
information.

## Acknowledgements

This project is in no way endorsed by the Klipper project. Please do not direct
any support requests to the Klipper project.

  * [Klipper](https://www.klipper3d.org/) by [Kevin O'Connor](https://www.patreon.com/koconnor)
