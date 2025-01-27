#[repr(u16)]
enum EmergencyClass {
    ErrorResetOrNoError = 0x0000,
    GenericError = 0x1000,
    Current = 0x2000,
    CurrentCanopenDeviceInputSide = 0x2100,
    CurrentInsideTheCanopenDevice = 0x2200,
    CurrentCanopenDeviceOutputSide = 0x2300,
    Voltage = 0x3000,
    MainsVoltage = 0x3100,
    VoltageInsideTheCanopenDevice = 0x3200,
    OutputVoltage = 0x3300,
    Temperature = 0x4000,
    AmbientTemperature = 0x4100,
    CanopenDeviceTemperature = 0x4200,
    CanopenDeviceHardware = 0x5000,
    CanopenDeviceSoftware = 0x6000,
    InternalSoftware = 0x6100,
    UserSoftware = 0x6200,
    DataSet = 0x6300,
    AdditionalModules = 0x7000,
    Monitoring = 0x8000,
    Communication = 0x8100,
    ProtocolError = 0x8200,
    ExternalError = 0x9000,
    AdditionalFunctions = 0xF000,
    CanopenDeviceSpecific = 0xFF00,
}

impl Into<u16> for EmergencyClass {
    fn into(self) -> u16 {
        self as u16
    }
}

impl TryFrom<u16> for EmergencyClass {
    type Error = ();

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value & 0xFF00 {
            0x0000 => Ok(EmergencyClass::ErrorResetOrNoError),
            0x1000 => Ok(EmergencyClass::GenericError),
            0x2000 => Ok(EmergencyClass::Current),
            0x2100 => Ok(EmergencyClass::CurrentCanopenDeviceInputSide),
            0x2200 => Ok(EmergencyClass::CurrentInsideTheCanopenDevice),
            0x2300 => Ok(EmergencyClass::CurrentCanopenDeviceOutputSide),
            0x3000 => Ok(EmergencyClass::Voltage),
            0x3100 => Ok(EmergencyClass::MainsVoltage),
            0x3200 => Ok(EmergencyClass::VoltageInsideTheCanopenDevice),
            0x3300 => Ok(EmergencyClass::OutputVoltage),
            0x4000 => Ok(EmergencyClass::Temperature),
            0x4100 => Ok(EmergencyClass::AmbientTemperature),
            0x4200 => Ok(EmergencyClass::CanopenDeviceTemperature),
            0x5000 => Ok(EmergencyClass::CanopenDeviceHardware),
            0x6000 => Ok(EmergencyClass::CanopenDeviceSoftware),
            0x6100 => Ok(EmergencyClass::InternalSoftware),
            0x6200 => Ok(EmergencyClass::UserSoftware),
            0x6300 => Ok(EmergencyClass::DataSet),
            0x7000 => Ok(EmergencyClass::AdditionalModules),
            0x8000 => Ok(EmergencyClass::Monitoring),
            0x8100 => Ok(EmergencyClass::Communication),
            0x8200 => Ok(EmergencyClass::ProtocolError),
            0x9000 => Ok(EmergencyClass::ExternalError),
            0xF000 => Ok(EmergencyClass::AdditionalFunctions),
            0xFF00 => Ok(EmergencyClass::CanopenDeviceSpecific),
            _ => Err(()),
        }
    }
}

/*

8100 h Communication – generic
8110 h CAN overrun (objects lost)
8120 h CAN in error passive mode
8130 h Life guard error or heartbeat error
8140 h recovered from bus off
8150 h CAN-ID collision

8200 h Protocol error - generic
8210 h PDO not processed due to length error
8220 h PDO length exceeded
8230 h DAM MPDO not processed, destination object not available
8240 h Unexpected SYNC data length
8250 h RPDO timeout

*/
