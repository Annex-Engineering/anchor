//! Anchor is an implementation of the Klipper firmware communication protocol
//!
//! Anchor can be used to implement custom MCUs without using the Klipper codebase as a starting
//! point. There can be many reasons to go this route, some include:
//!
//!   * Using Rust for firmware development
//!   * Licensing concerns
//!   * New MCU platforms
//!
//! Anchor implements only the MCU protocol, it is up to the user to implement the various
//! contracts required by e.g. Klippy. This allows great flexibility, as Anchor can be used over
//! USB, UART, virtual PTY, and potentially even others in the future.
//!
//! Anchor requires a custom build step. This is necessary for the Klipper protocol which needs a
//! data dictionary with global information about the protocol the MCU implements. Klippy will
//! query this with the `identify` command and expects a reply with the `identify_response`
//! reply. Anchor implements these two internally, but all other commands must be implemented by
//! the user.
//!
//! To get started, add the Anchor dependencies. This is done by adding the following to the
//! project `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! anchor = { git = "https://github.com/Annex-engineering/anchor.git" }
//!
//! [build-dependencies]
//! anchor_codegen = { git = "https://github.com/Annex-engineering/anchor.git" }
//! ```
//!
//! Once the dependencies have been added, include a custom build step. This is done by creating
//! (or modifying the existing) `build.rs` file. This file must exist in the same directory as
//! `Cargo.toml`. A minimal example:
//! ```
//! fn main() {
//!     anchor_codegen::ConfigBuilder::new()
//!         .entry("src/main.rs")
//!         .set_version("jig 0.1")
//!         .set_build_versions("rust: someversion")
//!         .build()
//! }
//! ```
//!
//! See `anchor_codegen::ConfigBuilder` for more information on supported build step options.
//!
//! With the build step in place, Anchor can be hooked in to the existing code base. Assuming some
//! form of communcation e.g. USB or UART is already available, this is a two part process: hooking
//! up the input path, and hooking up the output path.
//!
//! To start, hook up the output path. This is done by creating a [`TransportOutput`]
//! implementation, which will then be supplied to [`klipper_config_generate`]. This will by
//! necessity be specific to your project. An example implementation for a USB-based communication
//! channel could be:
//! ```
//! pub static USB_TX_BUFFER: Mutex<RefCell<FifoBuffer<{ USB_MAX_PACKET_SIZE * 2 }>>> =
//!     Mutex::new(RefCell::new(FifoBuffer::new()));
//! pub(crate) struct BufferTransportOutput;
//!
//! impl TransportOutput for BufferTransportOutput {
//!     type Output = ScratchOutput;
//!     fn output(&self, f: impl FnOnce(&mut Self::Output)) {
//!         let mut scratch = ScratchOutput::new();
//!         f(&mut scratch);
//!         let output = scratch.result();
//!         free(|cs| USB_TX_BUFFER.borrow(cs).borrow_mut().extend(output));
//!     }
//! }
//!
//! pub(crate) const TRANSPORT_OUTPUT: BufferTransportOutput = BufferTransportOutput;
//! ```
//!
//! The above code implements [`TransportOutput`] by having a globally shared buffer that can be
//! appended to. The callback is passed a `ScratchOutput` to fill, after which the output is copied
//! to the global buffer with interrupts disabled.
//!
//! Note that in the example code above, no actual transmission is done. Instead, data is added to
//! a buffer. This buffer will be flushed to the USB channel at a later time by the main loop.
//!
//! With the [`TransportOutput`] ready, add the [`klipper_config_generate!`] invocation. Usually
//! this is best done in the `main.rs` file of the project:
//! ```
//! klipper_config_generate!(
//!   transport = crate::usb::TRANSPORT_OUTPUT: crate::usb::BufferTransportOutput,
//! );
//! ```
//!
//! To pass a context to all command handlers, set the `context` parameter of. See the
//! documentation for [`klipper_config_generate`].
//!
//! With the output set up, the receive side can be hooked up. The [`klipper_config_generate`] call
//! generates a `KLIPPER_TRANSPORT` global constant, of type [`Transport`]. To parse incoming
//! commands, pass received bytes to the `receive` method of this. As with the write path, this
//! will be specific to your project. An example implementation could be:
//! ```
//! // Pump USB read side
//! let recv_data = receive_buffer.data();
//! if !recv_data.is_empty() {
//!     let mut wrap = SliceInputBuffer::new(recv_data);
//!     KLIPPER_TRANSPORT.receive(&mut wrap, &mut self.state);
//!     let consumed = recv_data.len() - wrap.available();
//!     if consumed > 0 {
//!         receive_buffer.pop(consumed);
//!     }
//! }
//! ```
//!
//! The data buffer must be wrapped in an [`InputBuffer`] implementation. When `receive` returns,
//! the data it consumed from the start of [`SliceInputBuffer`] can be safely removed from the
//! start of receive buffer. No data must be removed from the start of the receive buffer until
//! this happens. No buffering is implemented within `receive`, it is the responsibility of the
//! caller to maintain the input buffer.
//!
//! With this, Anchor is hooked up and Klipper message handlers, commands, enumerations, and
//! constants can be added as required. See the macros in this crate for more information.
//!
//! Anchor itself implements no commands except `identify` and `identify_response`. It is up to the
//! user to fulfill all relevant protocols. For examples, see the `testjig` example project.  
//! At the very least, you'll want to implement the following commands:
//!
//! | Command          | Note                       |
//! |------------------|----------------------------|
//! | `get_uptime`     | Must respond with `uptime` |
//! | `get_clock`      | Must respond with `clock`  |
//! | `emergency_stop` | Can be a no-op             |
//! | `allocate_oids`  | Can be a no-op             |
//! | `get_config`     | Must reply with `config`   |
//! | `config_reset`   | See example                |
//! | `finalize_config`| See example                |

#![cfg_attr(not(feature = "std"), no_std)]

#[doc(hidden)]
pub mod encoding;
#[doc(hidden)]
pub mod input_buffer;
#[doc(hidden)]
pub mod output_buffer;

#[doc(hidden)]
pub mod transport;
#[doc(hidden)]
pub mod transport_output;

mod fifo_buffer;

pub use anchor_macro::*;
pub use fifo_buffer::FifoBuffer;
pub use input_buffer::{InputBuffer, SliceInputBuffer};
pub use output_buffer::{OutputBuffer, ScratchOutput};
pub use transport::Transport;
pub use transport_output::TransportOutput;
