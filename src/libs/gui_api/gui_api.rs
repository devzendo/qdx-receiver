// -------------------------------------------------------------------------------------------------
// GRAPHICAL USER INTERFACE API
// -------------------------------------------------------------------------------------------------

use std::sync::Arc;
use std::sync::mpsc::SyncSender;

// The Receiver can effect changes in parts of the GUI by sending messages of this type
// to the GUIInput channel (sender), obtained from the GUI.
#[derive(Clone, PartialEq, Copy)]
pub enum GUIInputMessage {
    SignalStrength(f32)
}

// The Receiver can connect to the GUI by implementing this, and sending these messages.
pub trait GUIInput {
    fn set_gui_input(&mut self, gui_input: Arc<SyncSender<GUIInputMessage>>);
}

// Internal GUI messaging
#[derive(Clone, Debug)]
pub enum Message {
    SetAmplitude(f32),
    SignalStrength(f32),
    IncrementFrequencyDigit(u32),
    DecrementFrequencyDigit(u32),
    SetBandMetres(u8),
    ToggleMute,
}

// The GUI controls can effect changes in the rest of the system via this facade...
// ... which is implemented by the Receiver.
pub trait GUIOutput {
    fn set_frequency(&mut self, frequency_hz: u32);
    fn set_amplitude(&mut self, amplitude: f32); // 0.0 -> 1.0
}
