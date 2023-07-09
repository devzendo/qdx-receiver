#[macro_use]
extern crate clap;
extern crate portaudio;

use std::error::Error;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;
use std::{env, thread};
use std::time::Duration;

use clap::{App, Arg, ArgMatches};
use clap::arg_enum;
use fltk::app;
use log::{debug, error, info, warn};
use fltk::app::*;
use portaudio::{InputStreamSettings, OutputStreamSettings, PortAudio};
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

fn run(arguments: ArgMatches, mode: Mode) -> Result<i32, Box<dyn Error>> {
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
