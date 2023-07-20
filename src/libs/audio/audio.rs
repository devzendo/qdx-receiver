// -------------------------------------------------------------------------------------------------
// AUDIO INTERFACING
// -------------------------------------------------------------------------------------------------

use std::error::Error;
use log::info;
use portaudio as pa;
use portaudio::{InputStreamSettings, OutputStreamSettings, PortAudio};
use portaudio::stream::Parameters;

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
        let is_qdx_input = in_channels == 2 && in_48k_supported && info.name.contains("QDX");
        if is_qdx_input {
            info!("Using {:?} as QDX input device", info);
            let settings = InputStreamSettings::new(input_params, SAMPLE_RATE, FRAMES_PER_BUFFER);
            return Ok((settings, input_params));
        }
    }
    Err(Box::<dyn Error + Send + Sync>::from("Can't find QDX input device"))
}

pub fn is_speaker_name(x: &str) -> bool {
    x.eq_ignore_ascii_case("built-in output") || x.eq_ignore_ascii_case("macbook pro speakers") ||
        x.eq_ignore_ascii_case("speakers (realtek high definition audio")
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
    Err(Box::<dyn Error + Send + Sync>::from("Can't find speaker output device"))
}
