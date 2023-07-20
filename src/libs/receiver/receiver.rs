// -------------------------------------------------------------------------------------------------
// RECEIVER
// -------------------------------------------------------------------------------------------------

use std::error::Error;
use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;
use log::{debug, info, warn};
use portaudio::{Duplex, DuplexStreamSettings, NonBlocking, PortAudio, Stream};
use portaudio as pa;
use crate::libs::cat::cat::Cat;
use crate::libs::gui_api::gui_api::{GUIInput, GUIInputMessage, GUIOutput};

#[derive(Clone)]
pub struct CallbackData {
    amplitude: f32,
    avg_waveform_amplitude: f32,
}

pub struct Receiver {
    gui_input: Arc<Mutex<Option<Arc<SyncSender<GUIInputMessage>>>>>,
    read_thread_handle: Mutex<Option<JoinHandle<()>>>,
    duplex_stream: Option<Stream<NonBlocking, Duplex<f32, f32>>>,
    callback_data: Arc<RwLock<CallbackData>>,
    cat: Arc<Mutex<Cat>>,
}

// TODO replace this with obtaining the audio gain from the QDX, and setting it directly.
const AMPLITUDE_GAIN: f32 = 20.0;
// Thanks to MBo: https://stackoverflow.com/questions/55016337/calculate-or-update-average-without-iteration-over-time
const ALPHA: f32 = 0.1;

impl Receiver {
    pub fn new(terminate: Arc<AtomicBool>, cat: Arc<Mutex<Cat>>) -> Self {
        let callback_data = CallbackData {
            amplitude: 0.0,
            avg_waveform_amplitude: 0.0,
        };

        let arc_lock_callback_data = Arc::new(RwLock::new(callback_data));
        let gui_input_holder: Arc<Mutex<Option<Arc<SyncSender<GUIInputMessage>>>>> = Arc::new(Mutex::new(None));
        let thread_gui_input_holder = gui_input_holder.clone();

        // This thread periodically sends the avg_waveform_amplitude to the gui_input.
        let thread_callback_data = arc_lock_callback_data.clone();
        let read_thread_handle = thread::spawn(move || {
            loop {
                if terminate.load(Ordering::SeqCst) {
                    info!("Terminating FakeReceiver thread");
                    break;
                }
                thread::sleep(Duration::from_millis(250));
                let sender = thread_gui_input_holder.lock().unwrap();
                match sender.as_deref() {
                    None => {
                    }
                    Some(gui_input) => {
                        let callback_data = thread_callback_data.read().unwrap();
                        let strength = callback_data.avg_waveform_amplitude;
                        drop(callback_data);

                        gui_input.send(GUIInputMessage::SignalStrength(strength)).unwrap();
                    }
                }
            }
        });
        Self {
            gui_input: gui_input_holder,
            read_thread_handle: Mutex::new(Some(read_thread_handle)),
            duplex_stream: None,
            callback_data: arc_lock_callback_data,
            cat,
        }
    }

    // The odd form of this callback setup (pass in the PortAudio and settings) rather than just
    // returning the callback to the caller to do stuff with... is because I can't work out what
    // the correct type signature of a callback-returning function should be.
    pub fn start_duplex_callback(&mut self, pa: &PortAudio, duplex_settings: DuplexStreamSettings<f32, f32>) -> Result<(), Box<dyn Error>> {

        let move_clone_callback_data = self.callback_data.clone();

        let callback = move |pa::DuplexStreamCallbackArgs::<f32, f32> { in_buffer, out_buffer, frames, .. }| {
            //info!("input buffer length is {}, output buffer length is {}, frames is {}", in_buffer.len(), out_buffer.len(), frames);
            // input buffer length is 128, output buffer length is 128, frames is 64
            let callback_data = move_clone_callback_data.read().unwrap();
            let amplitude = callback_data.amplitude * AMPLITUDE_GAIN;
            drop(callback_data);

            let mut avg_waveform_amplitude = 0.0;
            for idx in 0..frames * 2 {
                // TODO MONO - if opening the stream with a single channel causes the same values to
                // be written to both left and right outputs, this could be optimised..

                out_buffer[idx] = in_buffer[idx] * amplitude; // why a scaling factor? why is input so quiet? don't know!
                avg_waveform_amplitude += in_buffer[idx];
            }

            avg_waveform_amplitude /= 128.0;
            let mut callback_data = move_clone_callback_data.write().unwrap();
            // Exponentially moving average
            callback_data.avg_waveform_amplitude = callback_data.avg_waveform_amplitude * (1.0-ALPHA) + avg_waveform_amplitude * ALPHA;

            pa::Continue
        };

        let maybe_stream = pa.open_non_blocking_stream(duplex_settings, callback);
        match maybe_stream {
            Ok(mut stream) => {
                info!("Starting duplex stream");
                stream.start()?;
                self.duplex_stream = Some(stream);
            }
            Err(e) => {
                warn!("Error opening duplex stream: {}", e);
            }
        }
        Ok(())
        // Now it's playing...
    }
}

impl GUIInput for Receiver {
    fn set_gui_input(&mut self, gui_input: Arc<SyncSender<GUIInputMessage>>) {
        *self.gui_input.lock().unwrap() = Some(gui_input);
    }
}

impl GUIOutput for Receiver {
    fn set_frequency(&mut self, frequency_hz: u32) {
        self.cat.lock().unwrap().set_frequency(frequency_hz).unwrap();
    }

    fn set_amplitude(&mut self, amplitude: f32) {
        let mut callback_data = self.callback_data.write().unwrap();
        callback_data.amplitude = amplitude;
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        info!("Stopping duplex stream: {:?}", self.duplex_stream.as_mut().unwrap().stop());
        debug!("Receiver joining thread handle...");
        let mut read_thread_handle = self.read_thread_handle.lock().unwrap();
        read_thread_handle.take().map(JoinHandle::join);
        debug!("...FakeReceiver joined thread handle");
    }
}
