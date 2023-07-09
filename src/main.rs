#[macro_use]
extern crate clap;
extern crate portaudio;

use std::error::Error;
use std::sync::{Arc, RwLock};
use std::{env, thread};
use std::f32::consts::PI;
use std::time::Duration;

use clap::{App, Arg, ArgMatches};
use clap::arg_enum;
use fltk::app;
use log::{debug, error, info, warn};
use fltk::app::*;
use portaudio::{InputStreamSettings, NonBlocking, Output, OutputStreamSettings, PortAudio, Stream};
use portaudio as pa;

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

pub fn get_qdx_input_device(pa: &PortAudio) -> Result<InputStreamSettings<f32>, Box<dyn Error>> {
    for device in pa.devices()? {
        let (idx, info) = device?;

        let in_channels = info.max_input_channels;
        let input_params = pa::StreamParameters::<f32>::new(idx, in_channels, INTERLEAVED, LATENCY);
        let in_48k_supported = pa.is_input_format_supported(input_params, SAMPLE_RATE).is_ok();
        let is_qdx_input = in_channels == 2 && in_48k_supported && info.name.find("QDX").is_some();
        if is_qdx_input {
            info!("Using {:?} as QDX input device", info);
            let settings = InputStreamSettings::new(input_params, SAMPLE_RATE, FRAMES_PER_BUFFER);
            return Ok(settings);
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

pub fn get_speaker_output_device(pa: &PortAudio) -> Result<OutputStreamSettings<f32>, Box<dyn Error>> {
    for device in pa.devices()? {
        let (idx, info) = device?;

        let out_channels = info.max_output_channels;
        let output_params =
            pa::StreamParameters::<f32>::new(idx, out_channels, INTERLEAVED, LATENCY);
        let out_48k_supported = pa.is_output_format_supported(output_params, SAMPLE_RATE).is_ok();
        if is_speaker_name(info.name) && out_channels == 2 && out_48k_supported {
            info!("Using {:?} as audio output device", info);
            let settings = OutputStreamSettings::new(output_params, SAMPLE_RATE, FRAMES_PER_BUFFER);
            return Ok(settings);
        }
    }
    Err(Box::<dyn Error + Send + Sync>::from(format!("Can't find speaker output device")))
}

#[derive(Clone)]
pub struct CallbackData {
    amplitude: f32,
    delta_phase: f32, // added to the phase after recording each sample
    phase: f32,       // sin(phase) is the sample value
}

struct Receiver {
    output_stream: Option<Stream<NonBlocking, Output<f32>>>,
    callback_data: Arc<RwLock<CallbackData>>,
    audio_frequency: u16,
}

impl Receiver {
    pub fn new(audio_frequency: u16) -> Self {
        let callback_data = CallbackData {
            amplitude: 0.5,
            delta_phase: 0.0,
            phase: 0.0,
        };

        let arc_lock_callback_data = Arc::new(RwLock::new(callback_data));
        Self {
            output_stream: None,
            callback_data: arc_lock_callback_data,
            audio_frequency,
        }
    }

    // The odd form of this callback setup (pass in the PortAudio and settings) rather than just
    // returning the callback to the caller to do stuff with... is because I can't work out what
    // the correct type signature of a callback-returning function should be.
    pub fn start_output_callback(&mut self, pa: &PortAudio, mut output_settings: OutputStreamSettings<f32>) -> Result<(), Box<dyn Error>> {
        let sample_rate = output_settings.sample_rate as u32;
        self.callback_data.write().unwrap().delta_phase = 2.0_f32 * PI * (self.audio_frequency as f32) / (sample_rate as f32);

        let move_clone_callback_data = self.callback_data.clone();

        let callback = move |pa::OutputStreamCallbackArgs::<f32> { buffer, frames, .. }| {
            //info!("buffer length is {}, frames is {}", buffer.len(), frames);
            // buffer length is 128, frames is 64; idx goes from [0..128).
            // One frame is a pair of left/right channel samples.
            // 48000/64=750 so in one second there are 48000 samples (frames), and 750 calls to this callback.
            // 1000/750=1.33333 so each buffer has a duration of 1.33333ms.
            let mut idx = 0;

            for _ in 0..frames {
                // The processing of amplitude/phase needs to be done every frame.
                let mut callback_data = move_clone_callback_data.write().unwrap();
                callback_data.phase += callback_data.delta_phase;
                let sine_val = f32::sin(callback_data.phase) * callback_data.amplitude;
                drop(callback_data);

                // TODO MONO - if opening the stream with a single channel causes the same values to
                // be written to both left and right outputs, this could be optimised..
                buffer[idx] = sine_val;
                buffer[idx + 1] = sine_val;

                idx += 2;
            }
            // idx is 128...
            pa::Continue
        };

        // we won't output out of range samples so don't bother clipping them.
        output_settings.flags = pa::stream_flags::CLIP_OFF;

        let maybe_stream = pa.open_non_blocking_stream(output_settings, callback);
        match maybe_stream {
            Ok(mut stream) => {
                info!("Starting stream");
                stream.start()?;
                self.output_stream = Some(stream);
            }
            Err(e) => {
                warn!("Error opening output stream: {}", e);
            }
        }
        Ok(())
        // Now it's playing...
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        info!("Stopping output stream: {:?}", self.output_stream.as_mut().unwrap().stop());
    }
}

fn run(_arguments: ArgMatches, mode: Mode) -> Result<i32, Box<dyn Error>> {
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

    info!("Initialising QDX input device...");
    let _qdx_input = get_qdx_input_device(&pa)?;
    info!("Initialising speaker output device...");
    let speaker_output = get_speaker_output_device(&pa)?;

    let mut receiver = Receiver::new(600);
    info!("Starting output callback...");
    receiver.start_output_callback(&pa, speaker_output)?;

    info!("Sleeping 10s...");
    thread::sleep(Duration::from_secs(10));

    info!("Exiting");
    Ok(0)
}

fn main() {
    initialise_logging();

    let (arguments, mode) = parse_command_line();
    debug!("Command line parsed");

    if mode == Mode::GUI {
        let _app = app::App::default().with_scheme(Scheme::Gleam);
        // TODO this should be passed to the GUI code.
    }

    match run(arguments, mode.clone()) {
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

