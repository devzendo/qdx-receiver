// -------------------------------------------------------------------------------------------------
// GRAPHICAL USER INTERFACE
// -------------------------------------------------------------------------------------------------

use std::sync::{Arc, mpsc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::sync_channel;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;
use fltk::image::PngImage;
use fltk::{app::*, app, button::*, draw::*, enums::*, prelude::*, widget::*, window::*};
use fltk::output::Output;
use fltk::valuator::SliderType::Horizontal;
use fltk::valuator::ValueSlider;
use log::{debug, error, info};
use rust_embed::RustEmbed;
use crate::libs::gui_api::gui_api::{GUIInputMessage, GUIOutput, Message};

pub const WIDGET_PADDING: i32 = 10;

const METER_WIDTH: i32 = 300;
const METER_HEIGHT: i32 = 167;

const DIGIT_HEIGHT: i32 = 40;
const DIGIT_BUTTON_DIM: i32 = (DIGIT_HEIGHT / 2) + 2;
const DIGIT_BUTTON_OFFSET: i32 = 4;

const BAND_BUTTON_DIM: i32 = (DIGIT_HEIGHT / 2) + 10;

const MUTE_BUTTON_DIM: i32 = (DIGIT_HEIGHT / 2) + 12;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Asset;

pub struct Gui {
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
    signal_strength: Arc<Mutex<f32>>,
    wheel_digit: Option<u32>,
}

impl Gui {
    pub fn new(version: &str, gui_output: Arc<Mutex<dyn GUIOutput>>, terminate: Arc<AtomicBool>, frequency: u32, amplitude: f32) -> Self {
        debug!("Initialising Window");
        let mut wind = Window::default().with_label(format!("qdx-receiver v{} de M0CUV", version).as_str());
        let window_background = Color::from_hex_str("#dfe2ff").unwrap();
        let meter_png_file = Asset::get("s-meter.png").unwrap().data;
        let mut meter_png = PngImage::from_data(&meter_png_file).unwrap();
        let thread_terminate = terminate.clone();
        let (gui_input_tx, gui_input_rx) = sync_channel::<GUIInputMessage>(16);

        let (sender, receiver) = channel::<Message>();
        let volume_sender_clone = sender.clone();
        let mouse_wheel_sender_clone = sender.clone();
        wind.handle(move |_w, ev| {
            if ev == Event::MouseWheel {
                let dy = app::event_dy();
                let message = if dy == MouseWheel::Down {
                    Message::DecrementFrequencyWheel
                } else {
                    Message::IncrementFrequencyWheel
                };
                mouse_wheel_sender_clone.send(message);
            }
            false
        });

        let up_button_y = WIDGET_PADDING + METER_HEIGHT + WIDGET_PADDING;
        let dn_button_y = WIDGET_PADDING + METER_HEIGHT + WIDGET_PADDING + DIGIT_BUTTON_DIM + WIDGET_PADDING + DIGIT_HEIGHT + WIDGET_PADDING;
        let updn_button_x = WIDGET_PADDING + DIGIT_BUTTON_OFFSET;
        let band_button_y = WIDGET_PADDING + METER_HEIGHT + WIDGET_PADDING + DIGIT_BUTTON_DIM + WIDGET_PADDING + DIGIT_HEIGHT + WIDGET_PADDING + DIGIT_BUTTON_DIM + WIDGET_PADDING;
        let volume_row_y = WIDGET_PADDING + METER_HEIGHT + WIDGET_PADDING + DIGIT_BUTTON_DIM + WIDGET_PADDING + DIGIT_HEIGHT + WIDGET_PADDING + DIGIT_BUTTON_DIM + WIDGET_PADDING + BAND_BUTTON_DIM + WIDGET_PADDING;

        let arc_mutex_signal_strength = Arc::new(Mutex::new(0.0));
        let meter_arc_mutex_signal_strength = arc_mutex_signal_strength.clone();
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
            signal_strength: arc_mutex_signal_strength,
            wheel_digit: None,
        };

        gui.meter_canvas.set_trigger(CallbackTrigger::Release);
        gui.meter_canvas.draw(move |wid| {
            let signal_strength = *meter_arc_mutex_signal_strength.lock().unwrap();
            Self::draw_meter(wid, signal_strength, &mut meter_png);
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
                            //info!("Signal strength is {:1.3}", amplitude);
                            thread_gui_sender.send(Message::SignalStrength(amplitude));
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

    fn draw_meter_line(theta: f32, long_r: f32, short_r: f32, mid_x: i32, mid_y: i32) {
        draw_line(mid_x - (long_r * theta.cos()) as i32,
                  mid_y - (long_r * theta.sin()) as i32,
                  mid_x - (short_r * theta.cos()) as i32,
                  mid_y - (short_r * theta.sin()) as i32);
    }

    fn draw_meter(wid: &mut Widget, signal_strength: f32, meter_png: &mut PngImage) {
        push_clip(wid.x(), wid.y(), wid.width(), wid.height());

        meter_png.draw(wid.x(), wid.y(), wid.width(), wid.height());
        set_draw_color(Color::Black);
        draw_rect(wid.x(), wid.y(), wid.width(), wid.height());


        // strength of 0 is 𝛉=5𝛑/6 (150º) = 2.6180, strength of 1 is 𝛉=𝛑/6 (30º) = 0.5236
        // difference is 2.0944. The end points are 'closer in' to the vertical axis than this so
        // reduce by this fudge..
        let theta_fudge = 0.28;
        let left_theta = 2.6180 - theta_fudge;
        let right_theta = 0.5236 + theta_fudge;
        let theta_range = left_theta - right_theta;
        // The meter is anchored at..
        let mid_x = (wid.width() / 2) + 10;
        let mid_y = wid.height() + 50; // the anchor of the needle is outside the clipping area

        // Draw needle
        set_line_style(LineStyle::Solid, 5);
        //let mut theta = right_theta;
        //loop {
        let fudged_signal_strength = signal_strength * 10.0; // to make it like my Yaesu :)
        let theta = fudged_signal_strength * theta_range + right_theta; // Theta increases from the right
        let long_r = 164.0;
        let short_r = 80.0;
        debug!("Updating meter to theta {} signal strength is {}", theta, fudged_signal_strength);
        Self::draw_meter_line(theta, long_r, short_r, mid_x, mid_y);
        //theta += 0.01;
        //if theta >= left_theta {
        //    break;
        //}}

        set_line_style(LineStyle::Solid, 0); // reset it, or everything is thick
        pop_clip();
    }

    fn show_frequency(&mut self) {
        self.frequency_output.set_value(format!("{:08}",self.frequency).as_str());
    }

    fn increment_digit(&mut self, digit: u32) {
        debug!("Previous frequency {}", self.frequency);
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
    
    fn decrement_digit(&mut self, digit: u32) {
        debug!("Previous frequency {}", self.frequency);
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

    pub fn message_handle(&mut self) {
        match self.receiver.recv() {
            None => {
                // noop
            }
            Some(message) => {
                debug!("App message {:?}", message);
                match message {
                    Message::SetAmplitude(amplitude) => {
                        info!("Setting amplitude to {}", amplitude);
                        self.gui_output.lock().unwrap().set_amplitude(amplitude);
                        self.amplitude = amplitude;
                    }
                    Message::IncrementFrequencyWheel => {
                        if let Some(digit) = self.wheel_digit {
                            self.increment_digit(digit);
                        }
                    }
                    Message::DecrementFrequencyWheel => {
                        if let Some(digit) = self.wheel_digit {
                            self.decrement_digit(digit);
                        }
                    }
                    Message::IncrementFrequencyDigit(digit) => {
                        self.wheel_digit = Some(digit);
                        self.increment_digit(digit);
                    }
                    Message::DecrementFrequencyDigit(digit) => {
                        self.wheel_digit = Some(digit);
                        self.decrement_digit(digit);
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
                    Message::SignalStrength(strength) => {
                        *self.signal_strength.lock().unwrap() = strength;
                        self.meter_canvas.redraw();
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
