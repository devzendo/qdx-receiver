// -------------------------------------------------------------------------------------------------
// FAKE RECEIVER for testing when QDX is not connected
// -------------------------------------------------------------------------------------------------

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;
use log::{debug, info};
use crate::libs::gui_api::gui_api::{GUIInput, GUIInputMessage, GUIOutput};

pub struct FakeReceiver {
    gui_input: Arc<Mutex<Option<Arc<SyncSender<GUIInputMessage>>>>>,
    read_thread_handle: Mutex<Option<JoinHandle<()>>>,
}

impl FakeReceiver {
    pub fn new(terminate: Arc<AtomicBool>) -> Self {
        let gui_input_holder: Arc<Mutex<Option<Arc<SyncSender<GUIInputMessage>>>>> = Arc::new(Mutex::new(None));
        let thread_gui_input_holder = gui_input_holder.clone();
        let read_thread_handle = thread::spawn(move || {
            let mut strength: f32 = 0.0;
            let mut strength_sign = 1.0;
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
                        gui_input.send(GUIInputMessage::SignalStrength(strength)).unwrap();
                        strength += 0.05 * strength_sign;
                        if strength < 0.0 {
                            strength = 0.0;
                            strength_sign = 1.0;
                        } else if strength > 1.0 {
                            strength = 1.0;
                            strength_sign = -1.0;
                        }
                    }
                }
            }
        });

        Self {
            gui_input: gui_input_holder,
            read_thread_handle: Mutex::new(Some(read_thread_handle)),
        }
    }
}

impl GUIInput for FakeReceiver {
    fn set_gui_input(&mut self, gui_input: Arc<SyncSender<GUIInputMessage>>) {
        *self.gui_input.lock().unwrap() = Some(gui_input);
    }
}

impl GUIOutput for FakeReceiver {
    fn set_frequency(&mut self, _frequency_hz: u32) {
    }

    fn set_amplitude(&mut self, _amplitude: f32) {
    }
}

impl Drop for FakeReceiver {
    fn drop(&mut self) {
        debug!("FakeReceiver joining thread handle...");
        let mut read_thread_handle = self.read_thread_handle.lock().unwrap();
        read_thread_handle.take().map(JoinHandle::join);
        debug!("...FakeReceiver joined thread handle");
    }
}
