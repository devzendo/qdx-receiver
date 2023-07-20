// -------------------------------------------------------------------------------------------------
// qdx-receiver
// (C) 2023 Matt Gumbley M0CUV
// -------------------------------------------------------------------------------------------------

#[macro_use]
extern crate clap;
extern crate portaudio;

use std::error::Error;
use std::sync::{Arc, Mutex};
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::{App, Arg, ArgMatches};
use clap::arg_enum;
use fltk::app;
use fltk::app::Scheme;
use log::{debug, error, info};
use portaudio::{DuplexStreamSettings, PortAudio};
use qdx_receiver::libs::audio::audio::{get_qdx_input_device, get_speaker_output_device, list_audio_devices};
use qdx_receiver::libs::cat::cat::Cat;
use qdx_receiver::libs::fakereceiver::fakereceiver::FakeReceiver;
use qdx_receiver::libs::gui::gui::Gui;
use qdx_receiver::libs::gui_api::gui_api::{GUIInput, GUIOutput};
use qdx_receiver::libs::receiver::receiver::Receiver;
use qdx_receiver::libs::serial::serial::find_qdx_serial_port;

// -------------------------------------------------------------------------------------------------
// COMMAND LINE HANDLING AND LOGGING
// -------------------------------------------------------------------------------------------------

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

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

const CAT_PORT_DEVICE: &str = "cat-port-device";
const AUDIO_OUT_DEVICE: &str = "audio-out-device";
const RIG_IN_DEVICE: &str = "rig-in-device";

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

    (result, mode)
}



pub const BUFFER_SIZE: usize = 128; // determined by watching what portaudio gives the callbacks.


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

    let frequency: u32;
    let receiver_gui_output: Arc<Mutex<dyn GUIOutput>>;
    let receiver_gui_input: Arc<Mutex<dyn GUIInput>>;

    let using_fake_receiver = false;
    if using_fake_receiver {
        frequency = 14074000;
        let fake_receiver_terminate = terminate.clone();
        let receiver = Arc::new(Mutex::new(FakeReceiver::new(fake_receiver_terminate)));
        receiver_gui_output = receiver.clone() as Arc<Mutex<dyn GUIOutput>>;
        receiver_gui_input = receiver.clone() as Arc<Mutex<dyn GUIInput>>;
    } else {
        info!("Initialising serial input device...");
        let serial_port = find_qdx_serial_port()?;
        let cat = Cat::new(serial_port.port_name)?;
        let arc_mutex_cat = Arc::new(Mutex::new(cat));

        frequency = arc_mutex_cat.lock().unwrap().get_frequency()?;
        info!("QDX on frequency at {:?}", frequency);

        info!("Initialising QDX input device...");
        let (_qdx_input, qdx_params) = get_qdx_input_device(&pa)?;
        info!("Initialising speaker output device...");
        let (_speaker_output, speaker_params) = get_speaker_output_device(&pa)?;

        pa.is_duplex_format_supported(qdx_params, speaker_params, 48000_f64)?;
        let duplex_settings = DuplexStreamSettings::new(qdx_params, speaker_params, 48000_f64, 64);

        let receiver = Arc::new(Mutex::new(Receiver::new(arc_mutex_cat)));
        receiver_gui_output = receiver.clone() as Arc<Mutex<dyn GUIOutput>>;
        receiver_gui_input = receiver.clone() as Arc<Mutex<dyn GUIInput>>;

        info!("Starting duplex callback...");
        receiver.lock().unwrap().start_duplex_callback(&pa, duplex_settings)?;
    }

    let mut gui = Gui::new(VERSION, receiver_gui_output, gui_terminate, frequency, amplitude);
    let gui_input = gui.gui_input_sender();
    receiver_gui_input.lock().unwrap().set_gui_input(gui_input);

    info!("Start of app wait loop");
    while app.unwrap().wait() {
        gui.message_handle();
    }
    info!("End of app wait loop");
    terminate.store(true, Ordering::SeqCst);
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

    match run(arguments, mode, app) {
        Err(err) => {
            match mode {
                Mode::GUI => {
                    fltk::dialog::message_default(&format!("{}", err));
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
