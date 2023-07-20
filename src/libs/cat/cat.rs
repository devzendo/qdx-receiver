// -------------------------------------------------------------------------------------------------
// CAT - COMPUTER AIDED TRANSCEIVER
// -------------------------------------------------------------------------------------------------

use std::error::Error;
use std::time::Duration;
use log::{debug, info};
use regex::Regex;
use serialport::{DataBits, FlowControl, Parity, SerialPort, SerialPortSettings, StopBits};

pub struct Cat {
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
        Ok(())
    }
}

impl Drop for Cat {
    fn drop(&mut self) {
        info!("Flushing serial port");
        self.serial_port.flush().expect("Could not flush");
    }
}

