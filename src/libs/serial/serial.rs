// -------------------------------------------------------------------------------------------------
// SERIAL PORT
// -------------------------------------------------------------------------------------------------

use std::error::Error;
use log::{debug, info};
use serialport::{SerialPortInfo, SerialPortType};

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
                    info!("Found QDX Transceiver as {:?}", found);
                    return Ok(return_p);
                }
            }
            SerialPortType::PciPort => {}
            SerialPortType::BluetoothPort => {}
            SerialPortType::Unknown => {}
        }
    }
    Err(Box::<dyn Error + Send + Sync>::from("Can't find QDX USB serial device"))
}

