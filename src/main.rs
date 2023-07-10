// -------------------------------------------------------------------------------------------------
// qdx-receiver
// (C) 2023 Matt Gumbley M0CUV
// -------------------------------------------------------------------------------------------------

#[macro_use]
extern crate clap;
extern crate portaudio;

use std::error::Error;
use std::sync::{Arc, mpsc, Mutex, RwLock};
use std::{env, thread};
use std::io::Write;
use std::str::Utf8Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::thread::JoinHandle;
use std::time::Duration;

use clap::{App, Arg, ArgMatches};
use clap::arg_enum;
use fltk::app;
use log::{debug, error, info, warn};
use fltk::{
    app::*, button::*, draw::*, enums::*, /*menu::*,*/ prelude::*, /*valuator::*,*/ widget::*, window::*,
};
use fltk::output::Output;
use fltk::valuator::SliderType::Horizontal;
use fltk::valuator::ValueSlider;
use portaudio::{Duplex, DuplexStreamSettings, InputStreamSettings, NonBlocking, OutputStreamSettings, PortAudio, Stream};
use portaudio as pa;
use portaudio::stream::Parameters;
use regex::Regex;
use serialport::{DataBits, FlowControl, Parity, SerialPort, SerialPortInfo, SerialPortSettings, SerialPortType, StopBits};

// -------------------------------------------------------------------------------------------------
// COMMAND LINE HANDLING AND LOGGING
// -------------------------------------------------------------------------------------------------

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

// -------------------------------------------------------------------------------------------------
// SERIAL PORT
// -------------------------------------------------------------------------------------------------

pub fn find_qdx_serial_port() -> Result<SerialPortInfo, Box<dyn Error>> {
    let ports = serialport::available_ports()?;
    info!("Scanning serial ports...");
    for p in ports {
        debug!("Port {:?}", p);
        let return_p = p.clone();
        let match_p = p.clone();
        match match_p.port_type {
            SerialPortType::UsbPort(usb) => {
                if usb.product == Some("QDX Transceiver".to_string()) {
                    let found = return_p.clone();
                    let returned = return_p.clone();
                    info!("Found QDX Transceiver as {:?}", found);
                    return Ok(returned);
                }
            }
            SerialPortType::PciPort => {}
            SerialPortType::BluetoothPort => {}
            SerialPortType::Unknown => {}
        }
    }
    Err(Box::<dyn Error + Send + Sync>::from(format!("Can't find QDX USB serial device")))
}


// -------------------------------------------------------------------------------------------------
// CAT - COMPUTER AIDED TRANSCEIVER
// -------------------------------------------------------------------------------------------------

struct Cat {
    port_name: String,
    serial_port: Box<dyn SerialPort>,
}

impl Cat {
    pub fn new(port_name: String) -> Result<Cat, Box<dyn Error>> {
        info!("Opening serial port {}", port_name);
        let settings = SerialPortSettings {
            baud_rate: 38400, // it's irrelevant over USB
            data_bits: DataBits::Eight,
            flow_control: FlowControl::Hardware,
            parity: Parity::None,
            stop_bits: StopBits::One,
            timeout: Duration::from_millis(250)
        };
        return match serialport::open_with_settings(&port_name, &settings) {
            Ok(serial_port) => {
                info!("Port open");
                let cat = Self {
                    port_name,
                    serial_port,
                };
                Ok(cat)
            }
            Err(e) => {
                Err(Box::<dyn Error + Send + Sync>::from(format!("Failed to open serial port {}: {}", port_name, e)))
            }
        };
    }

    // Synchronous.. Expects request to be a valid ;-terminated CAT command; an Ok response contains
    // a valid ;-terminated CAT response.
    fn transact(&mut self, request: &str) -> Result<String, Box<dyn Error>> {
        self.send_request(request)?;
        self.receive_response(request)
    }

    fn send_request(&mut self, request: &str) -> Result<(), Box<dyn Error>> {
        let request_bytes = request.as_bytes();
        if request.len() < 3 {
            return Err(Box::<dyn Error + Send + Sync>::from(format!("A CAT request must be at least 3 characters long, this is {}", request_bytes.len())));
        }
        debug!("Sending CAT request '{}'", request);
        let result = self.serial_port.write(request_bytes)?;
        if result != request_bytes.len() {
            return Err(Box::<dyn Error + Send + Sync>::from(format!("Expected to write {} bytes to QDX; wrote {}", request_bytes.len(), result)));
        }
        Ok(())
    }

    // Precondition: send_request has been used to validate and send the request.
    fn receive_response(&mut self, request: &str) -> Result<String, Box<dyn Error>> {
        let request_bytes = request.as_bytes();

        let mut received: Vec<u8> = vec![];
        loop {
            let mut byte = [0u8; 1];
            match self.serial_port.read(&mut byte) {
                Ok(n) => {
                    assert!(n == 1);
                    debug!("Received CAT response byte '{}'", byte[0]);
                    received.push(byte[0]);
                    if byte[0] == b';' {
                        debug!("Received end of CAT response");
                        break;
                    }
                }
                Err(err) => {
                    return Err(Box::<dyn Error + Send + Sync>::from(format!("No response from QDX: {}", err)));
                }
            }
        }
        match std::str::from_utf8(received.as_slice()) {
            Ok(response) => {
                if response.len() < 2 {
                    Err(Box::<dyn Error + Send + Sync>::from(format!("QDX response was too short: '{}'", response)))
                } else {
                    let response_bytes = response.as_bytes();
                    if response_bytes[0] != request_bytes[0] && response_bytes[1] != request_bytes[1] {
                        Err(Box::<dyn Error + Send + Sync>::from(format!("QDX response did not match request: '{}'", response)))
                    } else {
                        let response_string = response.to_string();
                        debug!("Received CAT response '{}'", response_string);
                        Ok(response_string)
                    }
                }
            }
            Err(err) => {
                Err(Box::<dyn Error + Send + Sync>::from(format!("Could not convert QDX response to String: {}", err)))
            }
        }
    }

    pub fn get_frequency(&mut self) -> Result<u32, Box<dyn Error>> {
        let fa_response = self.transact("FA;")?;
        let fa_response_regex = Regex::new(r"FA(\d{11});").unwrap();
        match fa_response_regex.captures(fa_response.as_str()) {
            Some(captures) => {
                let freq_string = captures.get(1).unwrap().as_str();
                let freq = str::parse::<u32>(freq_string)?;
                debug!("get_frequency returning {}", freq);
                Ok(freq)
            },
            None => Err(Box::<dyn Error + Send + Sync>::from(format!("Unexpected 'frequency' response: '{}'", fa_response)))
        }
    }

    pub fn set_frequency(&mut self, frequency_hz: u32) -> Result<(), Box<dyn Error>> {
        self.send_request(format!("FA{};", frequency_hz).as_str())?;
    }
}

impl Drop for Cat {
    fn drop(&mut self) {
        info!("Flushing serial port");
        self.serial_port.flush().expect("Could not flush");
    }
}

// -------------------------------------------------------------------------------------------------
// AUDIO INTERFACING
// -------------------------------------------------------------------------------------------------

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
    gui_input: Option<Arc<SyncSender<GUIInputMessage>>>,
    cat: Arc<Mutex<Cat>>,
}

const AMPLITUDE_GAIN: f32 = 20.0;

impl Receiver {
    pub fn new(cat: Arc<Mutex<Cat>>) -> Self {
        let callback_data = CallbackData {
            amplitude: 0.0,
        };

        let arc_lock_callback_data = Arc::new(RwLock::new(callback_data));
        Self {
            duplex_stream: None,
            callback_data: arc_lock_callback_data,
            gui_input: None,
            cat,
        }
    }

    pub fn set_gui_input(&mut self, gui_input: Arc<SyncSender<GUIInputMessage>>) {
        self.gui_input = Some(gui_input);
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
    fn set_frequency(&mut self, frequency_hz: u32) {
        info!("set_frequency {}", frequency_hz);
        self.cat.lock().unwrap().set_frequency(frequency_hz);
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

// -------------------------------------------------------------------------------------------------
// GRAPHICAL USER INTERFACE
// -------------------------------------------------------------------------------------------------

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


pub const WIDGET_PADDING: i32 = 10;

const WIDGET_HEIGHT: i32 = 25;

const METER_WIDTH: i32 = 300;
const METER_HEIGHT: i32 = 200;

const DIGIT_HEIGHT: i32 = 40;
const DIGIT_BUTTON_DIM: i32 = (DIGIT_HEIGHT / 2) + 2;
const DIGIT_BUTTON_OFFSET: i32 = 4;

const BAND_BUTTON_DIM: i32 = (DIGIT_HEIGHT / 2) + 10;

const MUTE_BUTTON_DIM: i32 = (DIGIT_HEIGHT / 2) + 12;


struct Gui {
    gui_input_tx: Arc<mpsc::SyncSender<GUIInputMessage>>,
    gui_output: Arc<Mutex<dyn GUIOutput>>,
    sender: fltk::app::Sender<Message>,
    receiver: fltk::app::Receiver<Message>,
    thread_handle: Mutex<Option<JoinHandle<()>>>,
    window_width: i32,
    window_height: i32,

    meter_canvas: Widget,
    frequency: u32,
    frequency_output: Output,
    up_button_7: Button,
    up_button_6: Button,
    up_button_5: Button,
    up_button_4: Button,
    up_button_3: Button,
    up_button_2: Button,
    up_button_1: Button,
    up_button_0: Button,

    dn_button_7: Button,
    dn_button_6: Button,
    dn_button_5: Button,
    dn_button_4: Button,
    dn_button_3: Button,
    dn_button_2: Button,
    dn_button_1: Button,
    dn_button_0: Button,

    band_80_button: Button,
    band_60_button: Button,
    band_40_button: Button,
    band_30_button: Button,
    band_20_button: Button,
    band_17_button: Button,
    band_15_button: Button,
    band_12_button: Button,
    band_11_button: Button,
    band_10_button: Button,

    amplitude: f32,
    volume_slider: ValueSlider,
    muted: bool,
    mute_button: Button,
}

impl Gui {
    pub fn new(gui_output: Arc<Mutex<dyn GUIOutput>>, terminate: Arc<AtomicBool>, frequency: u32, amplitude: f32) -> Self {
        debug!("Initialising Window");
        let mut wind = Window::default().with_label(format!("qdx-receiver v{} de M0CUV", VERSION).as_str());
        let window_background = Color::from_hex_str("#dfe2ff").unwrap();
        let meter_canvas_background = Color::from_hex_str("#aab0cb").unwrap();

        let thread_terminate = terminate.clone();
        let (gui_input_tx, gui_input_rx) = sync_channel::<GUIInputMessage>(16);

        let (sender, receiver) = channel::<Message>();
        let volume_sender_clone = sender.clone();

        let up_button_y = WIDGET_PADDING + METER_HEIGHT + WIDGET_PADDING;
        let dn_button_y = WIDGET_PADDING + METER_HEIGHT + WIDGET_PADDING + DIGIT_BUTTON_DIM + WIDGET_PADDING + DIGIT_HEIGHT + WIDGET_PADDING;
        let updn_button_x = WIDGET_PADDING + DIGIT_BUTTON_OFFSET;
        let band_button_y = WIDGET_PADDING + METER_HEIGHT + WIDGET_PADDING + DIGIT_BUTTON_DIM + WIDGET_PADDING + DIGIT_HEIGHT + WIDGET_PADDING + DIGIT_BUTTON_DIM + WIDGET_PADDING;
        let volume_row_y = WIDGET_PADDING + METER_HEIGHT + WIDGET_PADDING + DIGIT_BUTTON_DIM + WIDGET_PADDING + DIGIT_HEIGHT + WIDGET_PADDING + DIGIT_BUTTON_DIM + WIDGET_PADDING + BAND_BUTTON_DIM + WIDGET_PADDING;

        let mut gui = Gui {
            gui_input_tx: Arc::new(gui_input_tx),
            gui_output,
            sender,
            receiver,
            thread_handle: Mutex::new(None),
            window_width: WIDGET_PADDING + METER_WIDTH + WIDGET_PADDING,
            window_height: WIDGET_PADDING + METER_HEIGHT + WIDGET_PADDING + DIGIT_BUTTON_DIM  + WIDGET_PADDING + DIGIT_HEIGHT + WIDGET_PADDING + DIGIT_BUTTON_DIM + WIDGET_PADDING + BAND_BUTTON_DIM + WIDGET_PADDING + MUTE_BUTTON_DIM + WIDGET_PADDING,

            meter_canvas: Widget::new(WIDGET_PADDING, WIDGET_PADDING, METER_WIDTH, METER_HEIGHT, ""),
            frequency,
            frequency_output: Output::default()
                .with_size(DIGIT_BUTTON_DIM * 8 + 8, DIGIT_HEIGHT)
                .with_pos(WIDGET_PADDING, WIDGET_PADDING + METER_HEIGHT + WIDGET_PADDING + DIGIT_BUTTON_DIM + WIDGET_PADDING),

            up_button_7: Button::default()
                .with_size(DIGIT_BUTTON_DIM, DIGIT_BUTTON_DIM)
                .with_pos(updn_button_x + (0 * DIGIT_BUTTON_DIM), up_button_y)
                .with_label("▲"),
            up_button_6: Button::default()
                .with_size(DIGIT_BUTTON_DIM, DIGIT_BUTTON_DIM)
                .with_pos(updn_button_x + (1 * DIGIT_BUTTON_DIM), up_button_y)
                .with_label("▲"),
            up_button_5: Button::default()
                .with_size(DIGIT_BUTTON_DIM, DIGIT_BUTTON_DIM)
                .with_pos(updn_button_x + (2 * DIGIT_BUTTON_DIM), up_button_y)
                .with_label("▲"),
            up_button_4: Button::default()
                .with_size(DIGIT_BUTTON_DIM, DIGIT_BUTTON_DIM)
                .with_pos(updn_button_x + (3 * DIGIT_BUTTON_DIM), up_button_y)
                .with_label("▲"),
            up_button_3: Button::default()
                .with_size(DIGIT_BUTTON_DIM, DIGIT_BUTTON_DIM)
                .with_pos(updn_button_x + (4 * DIGIT_BUTTON_DIM), up_button_y)
                .with_label("▲"),
            up_button_2: Button::default()
                .with_size(DIGIT_BUTTON_DIM, DIGIT_BUTTON_DIM)
                .with_pos(updn_button_x + (5 * DIGIT_BUTTON_DIM), up_button_y)
                .with_label("▲"),
            up_button_1: Button::default()
                .with_size(DIGIT_BUTTON_DIM, DIGIT_BUTTON_DIM)
                .with_pos(updn_button_x + (6 * DIGIT_BUTTON_DIM), up_button_y)
                .with_label("▲"),
            up_button_0: Button::default()
                .with_size(DIGIT_BUTTON_DIM, DIGIT_BUTTON_DIM)
                .with_pos(updn_button_x + (7 * DIGIT_BUTTON_DIM), up_button_y)
                .with_label("▲"),

            dn_button_7: Button::default()
                .with_size(DIGIT_BUTTON_DIM, DIGIT_BUTTON_DIM)
                .with_pos(updn_button_x + (0 * DIGIT_BUTTON_DIM), dn_button_y)
                .with_label("▼"),
            dn_button_6: Button::default()
                .with_size(DIGIT_BUTTON_DIM, DIGIT_BUTTON_DIM)
                .with_pos(updn_button_x + (1 * DIGIT_BUTTON_DIM), dn_button_y)
                .with_label("▼"),
            dn_button_5: Button::default()
                .with_size(DIGIT_BUTTON_DIM, DIGIT_BUTTON_DIM)
                .with_pos(updn_button_x + (2 * DIGIT_BUTTON_DIM), dn_button_y)
                .with_label("▼"),
            dn_button_4: Button::default()
                .with_size(DIGIT_BUTTON_DIM, DIGIT_BUTTON_DIM)
                .with_pos(updn_button_x + (3 * DIGIT_BUTTON_DIM), dn_button_y)
                .with_label("▼"),
            dn_button_3: Button::default()
                .with_size(DIGIT_BUTTON_DIM, DIGIT_BUTTON_DIM)
                .with_pos(updn_button_x + (4 * DIGIT_BUTTON_DIM), dn_button_y)
                .with_label("▼"),
            dn_button_2: Button::default()
                .with_size(DIGIT_BUTTON_DIM, DIGIT_BUTTON_DIM)
                .with_pos(updn_button_x + (5 * DIGIT_BUTTON_DIM), dn_button_y)
                .with_label("▼"),
            dn_button_1: Button::default()
                .with_size(DIGIT_BUTTON_DIM, DIGIT_BUTTON_DIM)
                .with_pos(updn_button_x + (6 * DIGIT_BUTTON_DIM), dn_button_y)
                .with_label("▼"),
            dn_button_0: Button::default()
                .with_size(DIGIT_BUTTON_DIM, DIGIT_BUTTON_DIM)
                .with_pos(updn_button_x + (7 * DIGIT_BUTTON_DIM), dn_button_y)
                .with_label("▼"),

            band_80_button: Button::default()
                .with_size(BAND_BUTTON_DIM, BAND_BUTTON_DIM)
                .with_pos(WIDGET_PADDING + (0 * BAND_BUTTON_DIM), band_button_y)
                .with_label("80"),
            band_60_button: Button::default()
                .with_size(BAND_BUTTON_DIM, BAND_BUTTON_DIM)
                .with_pos(WIDGET_PADDING + (1 * BAND_BUTTON_DIM), band_button_y)
                .with_label("60"),
            band_40_button: Button::default()
                .with_size(BAND_BUTTON_DIM, BAND_BUTTON_DIM)
                .with_pos(WIDGET_PADDING + (2 * BAND_BUTTON_DIM), band_button_y)
                .with_label("40"),
            band_30_button: Button::default()
                .with_size(BAND_BUTTON_DIM, BAND_BUTTON_DIM)
                .with_pos(WIDGET_PADDING + (3 * BAND_BUTTON_DIM), band_button_y)
                .with_label("30"),
            band_20_button: Button::default()
                .with_size(BAND_BUTTON_DIM, BAND_BUTTON_DIM)
                .with_pos(WIDGET_PADDING + (4 * BAND_BUTTON_DIM), band_button_y)
                .with_label("20"),
            band_17_button: Button::default()
                .with_size(BAND_BUTTON_DIM, BAND_BUTTON_DIM)
                .with_pos(WIDGET_PADDING + (5 * BAND_BUTTON_DIM), band_button_y)
                .with_label("17"),
            band_15_button: Button::default()
                .with_size(BAND_BUTTON_DIM, BAND_BUTTON_DIM)
                .with_pos(WIDGET_PADDING + (6 * BAND_BUTTON_DIM), band_button_y)
                .with_label("15"),
            band_12_button: Button::default()
                .with_size(BAND_BUTTON_DIM, BAND_BUTTON_DIM)
                .with_pos(WIDGET_PADDING + (7 * BAND_BUTTON_DIM), band_button_y)
                .with_label("12"),
            band_11_button: Button::default()
                .with_size(BAND_BUTTON_DIM, BAND_BUTTON_DIM)
                .with_pos(WIDGET_PADDING + (8 * BAND_BUTTON_DIM), band_button_y)
                .with_label("11"),
            band_10_button: Button::default()
                .with_size(BAND_BUTTON_DIM, BAND_BUTTON_DIM)
                .with_pos(WIDGET_PADDING + (9 * BAND_BUTTON_DIM), band_button_y)
                .with_label("10"),

            amplitude,
            volume_slider: ValueSlider::default()
                .with_size(METER_WIDTH - WIDGET_PADDING - MUTE_BUTTON_DIM - WIDGET_PADDING, MUTE_BUTTON_DIM)
                .with_pos(WIDGET_PADDING, volume_row_y),
            muted: false,
            mute_button: Button::default()
                .with_size(MUTE_BUTTON_DIM, MUTE_BUTTON_DIM)
                .with_pos(WIDGET_PADDING + METER_WIDTH - MUTE_BUTTON_DIM, volume_row_y)
                .with_label("🔇"),
        };

        gui.meter_canvas.set_trigger(CallbackTrigger::Release);
        gui.meter_canvas.draw(move |wid| {
            push_clip(wid.x(), wid.y(), wid.width(), wid.height());
            draw_rect_fill(wid.x(), wid.y(), wid.width(), wid.height(), meter_canvas_background);

            set_draw_color(Color::Black);
            draw_rect(wid.x(), wid.y(), wid.width(), wid.height());
            pop_clip();
        });

        gui.frequency_output.set_color(window_background);
        gui.frequency_output.set_text_font(Font::CourierBold);
        gui.frequency_output.set_text_color(Color::Black);
        gui.frequency_output.set_text_size(36);
        gui.frequency_output.set_readonly(true);
        gui.show_frequency();

        gui.up_button_7.emit(gui.sender.clone(), Message::IncrementFrequencyDigit(7));
        gui.up_button_6.emit(gui.sender.clone(), Message::IncrementFrequencyDigit(6));
        gui.up_button_5.emit(gui.sender.clone(), Message::IncrementFrequencyDigit(5));
        gui.up_button_4.emit(gui.sender.clone(), Message::IncrementFrequencyDigit(4));
        gui.up_button_3.emit(gui.sender.clone(), Message::IncrementFrequencyDigit(3));
        gui.up_button_2.emit(gui.sender.clone(), Message::IncrementFrequencyDigit(2));
        gui.up_button_1.emit(gui.sender.clone(), Message::IncrementFrequencyDigit(1));
        gui.up_button_0.emit(gui.sender.clone(), Message::IncrementFrequencyDigit(0));
        gui.dn_button_7.emit(gui.sender.clone(), Message::DecrementFrequencyDigit(7));
        gui.dn_button_6.emit(gui.sender.clone(), Message::DecrementFrequencyDigit(6));
        gui.dn_button_5.emit(gui.sender.clone(), Message::DecrementFrequencyDigit(5));
        gui.dn_button_4.emit(gui.sender.clone(), Message::DecrementFrequencyDigit(4));
        gui.dn_button_3.emit(gui.sender.clone(), Message::DecrementFrequencyDigit(3));
        gui.dn_button_2.emit(gui.sender.clone(), Message::DecrementFrequencyDigit(2));
        gui.dn_button_1.emit(gui.sender.clone(), Message::DecrementFrequencyDigit(1));
        gui.dn_button_0.emit(gui.sender.clone(), Message::DecrementFrequencyDigit(0));

        gui.band_80_button.emit(gui.sender.clone(), Message::SetBandMetres(80));
        gui.band_60_button.emit(gui.sender.clone(), Message::SetBandMetres(60));
        gui.band_40_button.emit(gui.sender.clone(), Message::SetBandMetres(40));
        gui.band_30_button.emit(gui.sender.clone(), Message::SetBandMetres(30));
        gui.band_20_button.emit(gui.sender.clone(), Message::SetBandMetres(20));
        gui.band_17_button.emit(gui.sender.clone(), Message::SetBandMetres(17));
        gui.band_15_button.emit(gui.sender.clone(), Message::SetBandMetres(15));
        gui.band_12_button.emit(gui.sender.clone(), Message::SetBandMetres(12));
        gui.band_11_button.emit(gui.sender.clone(), Message::SetBandMetres(11));
        gui.band_10_button.emit(gui.sender.clone(), Message::SetBandMetres(10));

        gui.volume_slider.set_text_color(Color::Black);
        gui.volume_slider.set_bounds(0.0, 1.0);
        gui.volume_slider.set_value(gui.amplitude as f64);
        gui.volume_slider.set_type(Horizontal);
        gui.volume_slider.set_callback(move |wid| {
            volume_sender_clone.send(Message::SetAmplitude(wid.value() as f32));
        });
        // weird init needed..
        gui.sender.clone().send(Message::SetAmplitude(gui.amplitude));

        gui.mute_button.emit(gui.sender.clone(), Message::ToggleMute);
        gui.mute_button.set_color(Color::Light2);

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
                            info!("Signal strength is {}", amplitude);
                            // thread_gui_sender.send(Message::SetAmplitude(amplitude));
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

    fn show_frequency(&mut self) {
        self.frequency_output.set_value(format!("{:08}",self.frequency).as_str());
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
                        self.amplitude = amplitude;
                    }
                    Message::IncrementFrequencyDigit(digit) => {
                        info!("Previous frequency {}", self.frequency);
                        let pow = 10_u32.pow(digit);
                        if self.frequency + pow < 99999999 {
                            self.frequency += pow;
                            info!("New frequency {}", self.frequency);
                            self.gui_output.lock().unwrap().set_frequency(self.frequency);
                            self.show_frequency();
                        } else {
                            error!("Out of range!");
                        }
                    }
                    Message::DecrementFrequencyDigit(digit) => {
                        info!("Previous frequency {}", self.frequency);
                        let pow = 10_u32.pow(digit);
                        if self.frequency as i64 - pow as i64 >= 0 {
                            self.frequency -= pow;
                            info!("New frequency {}", self.frequency);
                            self.gui_output.lock().unwrap().set_frequency(self.frequency);
                            self.show_frequency();
                        } else {
                            error!("Out of range!");
                        }
                    }
                    Message::SetBandMetres(m) => {
                        info!("Setting band to {}m", m);
                        self.frequency = match m {
                            80 =>  3_573_000,
                            60 =>  5_357_000,
                            40 =>  7_074_000,
                            30 => 10_136_000,
                            20 => 14_074_000,
                            17 => 18_100_000,
                            15 => 21_074_000,
                            12 => 24_915_000,
                            11 => 27_255_000, // Maybe?
                            10 => 28_180_000,
                            _ => 14_074_000, // default to 20m
                        };
                        info!("New frequency {}", self.frequency);
                        self.gui_output.lock().unwrap().set_frequency(self.frequency);
                        self.show_frequency();
                    }
                    Message::ToggleMute => {
                        if self.muted {
                            info!("Unmuting with amplitude of {}", self.amplitude);
                            self.gui_output.lock().unwrap().set_amplitude(self.amplitude);
                            self.mute_button.set_color(Color::Light2);
                        } else {
                            info!("Muting");
                            self.gui_output.lock().unwrap().set_amplitude(0.0);
                            self.mute_button.set_color(Color::Red);
                        }
                        self.muted = !self.muted;
                    }
                }
            }
        }
    }

    // Use this to send update messages to the GUI.
    pub fn gui_input_sender(&self) -> Arc<mpsc::SyncSender<GUIInputMessage>> {
        self.gui_input_tx.clone()
    }
}

// -------------------------------------------------------------------------------------------------
// MAIN
// -------------------------------------------------------------------------------------------------

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

    let amplitude: f32 = 1.0; // Max; TODO take from config

    let pa = PortAudio::new()?;

    if mode == Mode::ListAudioDevices {
        list_audio_devices(&pa)?;
        return Ok(0)
    }

    let terminate = Arc::new(AtomicBool::new(false));
    let gui_terminate = terminate.clone();

    info!("Initialising serial input device...");
    let serial_port = find_qdx_serial_port()?;
    let mut cat = Cat::new(serial_port.port_name)?;
    let arc_mutex_cat = Arc::new(Mutex::new(cat));

    let frequency: u32 = arc_mutex_cat.lock().unwrap().get_frequency()?;
    info!("QDX on frequency at {:?}", frequency);

    info!("Initialising QDX input device...");
    let (_qdx_input, qdx_params) = get_qdx_input_device(&pa)?;
    info!("Initialising speaker output device...");
    let (_speaker_output, speaker_params) = get_speaker_output_device(&pa)?;

    pa.is_duplex_format_supported(qdx_params, speaker_params, 48000_f64)?;
    let duplex_settings = DuplexStreamSettings::new(qdx_params, speaker_params, 48000_f64, 64);

    let receiver = Arc::new(Mutex::new(Receiver::new(arc_mutex_cat.clone())));
    let receiver_gui_output: Arc<Mutex<dyn GUIOutput>> = receiver.clone() as Arc<Mutex<dyn GUIOutput>>;

    let mut gui = Gui::new(receiver_gui_output, gui_terminate, frequency, amplitude);
    let gui_input = gui.gui_input_sender();
    receiver.lock().unwrap().set_gui_input(gui_input);

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

// -------------------------------------------------------------------------------------------------
// FIN
// -------------------------------------------------------------------------------------------------
