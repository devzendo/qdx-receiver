#[macro_use]
extern crate clap;
extern crate portaudio;

use std::error::Error;
use std::sync::{Arc, mpsc, Mutex, RwLock};
use std::{env, thread};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::sync_channel;
use std::thread::JoinHandle;
use std::time::Duration;

use clap::{App, Arg, ArgMatches};
use clap::arg_enum;
use fltk::app;
use log::{debug, error, info, warn};
use fltk::app::*;
use fltk::enums::Color;
use fltk::prelude::{GroupExt, WidgetExt};
use fltk::window::Window;
use portaudio::{Duplex, DuplexStreamSettings, InputStreamSettings, NonBlocking, OutputStreamSettings, PortAudio, Stream};
use portaudio as pa;
use portaudio::stream::Parameters;

pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

pub fn initialise_logging() {
    let log_var_name = "RUST_LOG";
    if env::var(log_var_name).is_err() {
        env::set_var(log_var_name, "info")
    }
    env_logger::init();
}

#[cfg(windows)]
const CAT_HELP: &str = "Sets the port that the QDX CAT interface is available on, e.g. COM4:";
#[cfg(windows)]
const CAT_VALUE_NAME: &str = "COM port";

#[cfg(not(windows))]
const CAT_HELP: &str = "Sets the port that the QDX CAT interface is available on, e.g. /dev/cu-usbserial-1410";
#[cfg(not(windows))]
const CAT_VALUE_NAME: &str = "serial character device";

const CAT_PORT_DEVICE: &'static str = "cat-port-device";
const AUDIO_OUT_DEVICE: &'static str = "audio-out-device";
const RIG_IN_DEVICE: &'static str = "rig-in-device";

arg_enum! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum Mode {
        GUI,
        // ConfigFileLocation,
        ListAudioDevices,
    }
}

fn parse_command_line<'a>() -> (ArgMatches<'a>, Mode) {
    let result = App::new("qdx-receiver")
        .version(VERSION)
        .author("Matt Gumbley <matt.gumbley@gmail.com>")
        .about("QDX receiver application")

        .arg(Arg::from_usage("<mode> 'The mode to use, usually GUI.'").possible_values(&Mode::variants()).default_value("GUI"))

        .arg(Arg::with_name(CAT_PORT_DEVICE)
            .short("c")
            .long("catport")
            .value_name(CAT_VALUE_NAME)
            .help(CAT_HELP)
            .takes_value(true))

        .arg(Arg::with_name(AUDIO_OUT_DEVICE)
            .short("a").long("audioout").help("Sets the audio device name to use for the speaker/headphone output")
            .value_name("speaker/headphone audio output device name").takes_value(true))

        .arg(Arg::with_name(RIG_IN_DEVICE)
            .short("r").long("rigaudioin").help("Sets the audio device name to use for input from the transceiver")
            .value_name("transceiver audio input device name").takes_value(true))

        .get_matches();

    let mode = value_t!(result.value_of("mode"), Mode).unwrap_or(Mode::GUI);

    return (result, mode);
}

// PortAudio constants
const INTERLEAVED: bool = true;
const LATENCY: pa::Time = 0.0; // Ignored by PortAudio::is_*_format_supported.
pub(crate) const FRAMES_PER_BUFFER: u32 = 64; // May have to increase this to 1024
pub(crate) const SAMPLE_RATE: f64 = 48000.0;


pub fn list_audio_devices(pa: &PortAudio) -> Result<i32, Box<dyn Error>> {
    let num_devices = pa.device_count()?;
    info!("Number of audio devices = {}", num_devices);

    for device in pa.devices()? {
        let (idx, info) = device?;

        let in_channels = info.max_input_channels;
        let input_params = pa::StreamParameters::<i16>::new(idx, in_channels, INTERLEAVED, LATENCY);
        let out_channels = info.max_output_channels;
        let output_params =
            pa::StreamParameters::<f32>::new(idx, out_channels, INTERLEAVED, LATENCY);
        let in_48k_supported = pa.is_input_format_supported(input_params, SAMPLE_RATE).is_ok();
        let out_48k_supported = pa.is_output_format_supported(output_params, SAMPLE_RATE).is_ok();
        let support_48k = if (in_channels > 0 && in_48k_supported) || (out_channels > 0 && out_48k_supported) { "48000Hz supported" } else { "48000Hz not supported" };
        info!("{:?}: {:?} / IN:{} OUT:{} @ {}Hz default; {}", idx.0, info.name, info.max_input_channels,
            info.max_output_channels, info.default_sample_rate, support_48k);
    }
    Ok(0)
}

pub fn get_qdx_input_device(pa: &PortAudio) -> Result<(InputStreamSettings<f32>, Parameters<f32>), Box<dyn Error>> {
    for device in pa.devices()? {
        let (idx, info) = device?;

        let in_channels = info.max_input_channels;
        let input_params = pa::StreamParameters::<f32>::new(idx, in_channels, INTERLEAVED, LATENCY);
        let in_48k_supported = pa.is_input_format_supported(input_params, SAMPLE_RATE).is_ok();
        let is_qdx_input = in_channels == 2 && in_48k_supported && info.name.find("QDX").is_some();
        if is_qdx_input {
            info!("Using {:?} as QDX input device", info);
            let settings = InputStreamSettings::new(input_params, SAMPLE_RATE, FRAMES_PER_BUFFER);
            return Ok((settings, input_params));
        }
    }
    Err(Box::<dyn Error + Send + Sync>::from(format!("Can't find QDX input device")))
}

pub fn is_speaker_name(x: &str) -> bool {
    return x.eq_ignore_ascii_case("built-in output") || x.eq_ignore_ascii_case("macbook pro speakers") ||
        x.eq_ignore_ascii_case("speakers (realtek high definition audio");
    // a poor heuristic since there are several "realtek" devices, and the second one in the list
    // works - need to assess the DeviceInfo better on windows
}

pub fn get_speaker_output_device(pa: &PortAudio) -> Result<(OutputStreamSettings<f32>, Parameters<f32>), Box<dyn Error>> {
    for device in pa.devices()? {
        let (idx, info) = device?;

        let out_channels = info.max_output_channels;
        let output_params =
            pa::StreamParameters::<f32>::new(idx, out_channels, INTERLEAVED, LATENCY);
        let out_48k_supported = pa.is_output_format_supported(output_params, SAMPLE_RATE).is_ok();
        if is_speaker_name(info.name) && out_channels == 2 && out_48k_supported {
            info!("Using {:?} as audio output device", info);
            let settings = OutputStreamSettings::new(output_params, SAMPLE_RATE, FRAMES_PER_BUFFER);
            return Ok((settings, output_params));
        }
    }
    Err(Box::<dyn Error + Send + Sync>::from(format!("Can't find speaker output device")))
}

pub const BUFFER_SIZE: usize = 128; // determined by watching what portaudio gives the callbacks.

#[derive(Clone)]
pub struct CallbackData {
    amplitude: f32,
}

struct Receiver {
    duplex_stream: Option<Stream<NonBlocking, Duplex<f32, f32>>>,
    callback_data: Arc<RwLock<CallbackData>>,
}

impl Receiver {
    pub fn new() -> Self {

        let callback_data = CallbackData {
            amplitude: 20.0,
        };

        let arc_lock_callback_data = Arc::new(RwLock::new(callback_data));
        Self {
            duplex_stream: None,
            callback_data: arc_lock_callback_data,
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
            let amplitude = callback_data.amplitude;
            drop(callback_data);

            for idx in 0..frames * 2 {
                // TODO MONO - if opening the stream with a single channel causes the same values to
                // be written to both left and right outputs, this could be optimised..
                // callback_data.phase += callback_data.delta_phase;
                // let sine_val = f32::sin(callback_data.phase) * callback_data.amplitude;

                out_buffer[idx] = in_buffer[idx] * amplitude; // why a scaling factor? why is input so quiet? don't know!
            }

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

impl GUIOutput for Receiver {
    fn set_frequency(&mut self, _frequency_hz: u32) {
        error!("Unimplemented set_frequency");
    }

    fn set_amplitude(&mut self, amplitude: f32) {
        let mut callback_data = self.callback_data.write().unwrap();
        callback_data.amplitude = amplitude;
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        info!("Stopping duplex stream: {:?}", self.duplex_stream.as_mut().unwrap().stop());
    }
}

// The rest of the system can effect changes in parts of the GUI by sending messages of this type
// to the GUIInput channel (sender), obtained from the GUI.
#[derive(Clone, PartialEq, Copy)]
pub enum GUIInputMessage {
    SignalStrength(f32)
}

// Internal GUI messaging
#[derive(Clone, Debug)]
pub enum Message {
    SetAmplitude(f32),
}

// The GUI controls can effect changes in the rest of the system via this facade...
pub trait GUIOutput {
    fn set_frequency(&mut self, frequency_hz: u32);
    fn set_amplitude(&mut self, amplitude: f32); // 0.0 -> 1.0
}


pub const WIDGET_PADDING: i32 = 10;

const WIDGET_HEIGHT: i32 = 25;

const WATERFALL_WIDTH: i32 = 1000;
const WATERFALL_HEIGHT: i32 = 500;

// Central controls column
const CENTRAL_CONTROLS_WIDTH: i32 = 240;

struct Gui {
    gui_input_tx: Arc<mpsc::SyncSender<GUIInputMessage>>,
    gui_output: Arc<Mutex<dyn GUIOutput>>,
    sender: fltk::app::Sender<Message>,
    receiver: fltk::app::Receiver<Message>,
    thread_handle: Mutex<Option<JoinHandle<()>>>,
    window_width: i32,
    window_height: i32,

}

impl Gui {
    pub fn new(gui_output: Arc<Mutex<dyn GUIOutput>>, terminate: Arc<AtomicBool>) -> Self {
        debug!("Initialising Window");
        let mut wind = Window::default().with_label(format!("qdx-receiver v{} de M0CUV", VERSION).as_str());
        let window_background = Color::from_hex_str("#dfe2ff").unwrap();

        let thread_terminate = terminate.clone();
        let (gui_input_tx, gui_input_rx) = sync_channel::<GUIInputMessage>(16);

        let (sender, receiver) = channel::<Message>();
        let gui = Gui {
            gui_input_tx: Arc::new(gui_input_tx),
            gui_output,
            sender,
            receiver,
            window_width: WIDGET_PADDING + WATERFALL_WIDTH + WIDGET_PADDING + CENTRAL_CONTROLS_WIDTH + WIDGET_PADDING,
            window_height: WIDGET_PADDING + WATERFALL_HEIGHT + WIDGET_PADDING + WIDGET_HEIGHT + WIDGET_PADDING,
            thread_handle: Mutex::new(None),
        };
        wind.set_size(gui.window_width, gui.window_height);
        wind.set_color(window_background);

        // Functions called on the GUI by the rest of the system...
        let thread_gui_sender = gui.sender.clone();
        let thread_handle = thread::spawn(move || {
            loop {
                if thread_terminate.load(Ordering::SeqCst) {
                    info!("Terminating GUI input thread");
                    break;
                }

                if let Ok(gui_input_message) = gui_input_rx.recv_timeout(Duration::from_millis(250)) {
                    match gui_input_message {
                        GUIInputMessage::SignalStrength(amplitude) => {
                            thread_gui_sender.send(Message::SetAmplitude(amplitude));
                        }
                    }
                }
            }
        });

        *gui.thread_handle.lock().unwrap() = Some(thread_handle);

        wind.end();
        debug!("Showing main window");
        wind.show();
        debug!("Starting app wait loop");
        gui
    }

    pub fn message_handle(&mut self) {
        match self.receiver.recv() {
            None => {
                // noop
            }
            Some(message) => {
                info!("App message {:?}", message);
                match message {
                    Message::SetAmplitude(amplitude) => {
                        info!("Setting amplitude to {}", amplitude);
                        self.gui_output.lock().unwrap().set_amplitude(amplitude);
                    }
                }
            }
        }
    }

    pub fn gui_input_sender(&self) -> Arc<mpsc::SyncSender<GUIInputMessage>> {
        self.gui_input_tx.clone()
    }

}

fn run(_arguments: ArgMatches, mode: Mode, app: Option<fltk::app::App>) -> Result<i32, Box<dyn Error>> {
    // let home_dir = dirs::home_dir();
    // let config_path = config_dir::configuration_directory(home_dir)?;
    // let config_path_clone = config_path.clone();
    // let mut config = ConfigurationStore::new(config_path).unwrap();
    // let config_file_path = config.get_config_file_path();
    //
    // if mode == Mode::ConfigFileLocation {
    //     info!("Configuration path is [{:?}]", config_path_clone);
    //     info!("Configuration file is [{:?}]", config_file_path);
    //     return Ok(0)
    // }
    let pa = PortAudio::new()?;

    if mode == Mode::ListAudioDevices {
        list_audio_devices(&pa)?;
        return Ok(0)
    }

    let terminate = Arc::new(AtomicBool::new(false));
    let gui_terminate = terminate.clone();

    info!("Initialising QDX input device...");
    let (_qdx_input, qdx_params) = get_qdx_input_device(&pa)?;
    info!("Initialising speaker output device...");
    let (_speaker_output, speaker_params) = get_speaker_output_device(&pa)?;

    pa.is_duplex_format_supported(qdx_params, speaker_params, 48000_f64)?;
    let duplex_settings = DuplexStreamSettings::new(qdx_params, speaker_params, 48000_f64, 64);

    let receiver = Arc::new(Mutex::new(Receiver::new()));
    let receiver_gui_output: Arc<Mutex<dyn GUIOutput>> = receiver.clone() as Arc<Mutex<dyn GUIOutput>>;

    let mut gui = Gui::new(receiver_gui_output, gui_terminate);

    info!("Starting duplex callback...");
    receiver.lock().unwrap().start_duplex_callback(&pa, duplex_settings)?;

    info!("Start of app wait loop");
    while app.unwrap().wait() {
        gui.message_handle();
    }
    info!("End of app wait loop");

    info!("Exiting");
    Ok(0)
}

fn main() {
    initialise_logging();

    let (arguments, mode) = parse_command_line();
    debug!("Command line parsed");

    let mut app: Option<fltk::app::App> = None;
    if mode == Mode::GUI {
        app = Some(app::App::default().with_scheme(Scheme::Gleam));
    }

    match run(arguments, mode.clone(), app) {
        Err(err) => {
            match mode {
                Mode::GUI => {
                    fltk::dialog::message_default(&*format!("{}", err));
                }
                _ => {
                    error!("{}", err);
                }
            }
        }
        Ok(exit_code) => {
            std::process::exit(exit_code);
        }
    }
}

