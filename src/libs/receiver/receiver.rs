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
    min_waveform_amplitude: f32,
    max_waveform_amplitude: f32,
}

pub struct Receiver {
    gui_input: Arc<Mutex<Option<Arc<SyncSender<GUIInputMessage>>>>>,
    read_thread_handle: Mutex<Option<JoinHandle<()>>>,
    duplex_stream: Option<Stream<NonBlocking, Duplex<f32, f32>>>,
    callback_data: Arc<RwLock<CallbackData>>,
    cat: Arc<Mutex<Cat>>,
}

// TODO replace this with obtaining the audio gain from the QDX, and setting it directly.
const AMPLITUDE_GAIN: f32 = 90.0;

impl Receiver {
    pub fn new(terminate: Arc<AtomicBool>, cat: Arc<Mutex<Cat>>) -> Self {
        let callback_data = CallbackData {
            amplitude: 0.0,
            avg_waveform_amplitude: 0.0,
            min_waveform_amplitude: 100.0,
            max_waveform_amplitude: 0.0,
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
                thread::sleep(Duration::from_millis(100));
                let sender = thread_gui_input_holder.lock().unwrap();
                match sender.as_deref() {
                    None => {
                    }
                    Some(gui_input) => {
                        let callback_data = thread_callback_data.read().unwrap();
                        let strength = callback_data.avg_waveform_amplitude;
                        // info!("min {} max {}", callback_data.min_waveform_amplitude, callback_data.max_waveform_amplitude);
                        drop(callback_data);

                        let _ = gui_input.send(GUIInputMessage::SignalStrength(strength));
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
            let mut min_amp = 100.0;
            let mut max_amp = 0.0;
            for idx in 0..frames * 2 {
                // TODO MONO - if opening the stream with a single channel causes the same values to
                // be written to both left and right outputs, this could be optimised..
                let sample = in_buffer[idx] * amplitude;
                if sample < min_amp {
                    min_amp = sample;
                }
                if sample > max_amp {
                    max_amp = sample;
                }
                out_buffer[idx] = sample ; // why a scaling factor? why is input so quiet? don't know!
                avg_waveform_amplitude += sample.abs(); // Should be in range [0..1]
            }

            avg_waveform_amplitude /= 128.0; // should be in range [0..1]
            let mut callback_data = move_clone_callback_data.write().unwrap();
            callback_data.avg_waveform_amplitude -= callback_data.avg_waveform_amplitude / 40.0;
            callback_data.avg_waveform_amplitude += avg_waveform_amplitude / 40.0;
            if min_amp < callback_data.min_waveform_amplitude {
                callback_data.min_waveform_amplitude = min_amp;
            }
            if max_amp > callback_data.max_waveform_amplitude {
                callback_data.max_waveform_amplitude = max_amp;
            }

            // With AMPLITUDE set as above, the min/max are around -1 .. +1 on very strong signals.
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
