#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbortCode {
    ToggleBitNotAlternated = 0x05030000,
    SDOProtocolTimedOut = 0x05040000,
    ClientServerCommandSpecifierNotValidOrUnknown = 0x05040001,
    InvalidBlockSize = 0x05040002,
    InvalidSequenceNumber = 0x05040003,
    CrcError = 0x05040004,
    OutOfMemory = 0x05040005,
    UnsupportedAccessToAnObject = 0x06010000,
    AttemptToReadAWriteOnlyObject = 0x06010001,
    AttemptToWriteAReadOnlyObject = 0x06010002,
    ObjectDoesNotExistInTheObjectDictionary = 0x06020000,
    ObjectCannotBeMappedToThePDO = 0x06040041,
    TheNumberAndLengthOfTheObjectsToBeMappedWouldExceedPdoLength = 0x06040042,
    GeneralParameterIncompatibilityReason = 0x06040043,
    GeneralInternalIncompatibilityInTheDevice = 0x06040047,
    AccessFailedDueToAnHardwareError = 0x06060000,
    DataTypeDoesNotMatchLengthOfServiceParameterDoesNotMatch = 0x06070010,
    DataTypeDoesNotMatchLengthOfServiceParameterTooHigh = 0x06070012,
    DataTypeDoesNotMatchLengthOfServiceParameterTooLow = 0x06070013,
    SubIndexDoesNotExist = 0x06090011,
    InvalidValueForParameterDownloadOnly = 0x06090030,
    ValueOfParameterWrittenTooHighDownloadOnly = 0x06090031,
    ValueOfParameterWrittenTooLowDownloadOnly = 0x06090032,
    MaximumValueIsLessThanMinimumValue = 0x06090036,
    ResourceNotAvailableSdoConnection = 0x060A0023,
    GeneralError = 0x08000000,
    DataCannotBeTransferredOrStoredToTheApplication = 0x08000020,
    DataCannotBeTransferredOrStoredToTheApplicationBecauseOfLocalControl = 0x08000021,
    DataCannotBeTransferredOrStoredToTheApplicationBecauseOfThePresentDeviceState = 0x08000022,
    ObjectDictionaryDynamicGenerationFailsOrNoObjectDictionaryIsPresent = 0x08000023,
    NoDataAvailable = 0x08000024,
}

impl Into<u32> for AbortCode {
    fn into(self) -> u32 {
        self as u32
    }
}

impl From<u32> for AbortCode {
    fn from(value: u32) -> Self {
        match value {
            0x05030000 => AbortCode::ToggleBitNotAlternated,
            0x05040000 => AbortCode::SDOProtocolTimedOut,
            0x05040001 => AbortCode::ClientServerCommandSpecifierNotValidOrUnknown,
            0x05040002 => AbortCode::InvalidBlockSize,
            0x05040003 => AbortCode::InvalidSequenceNumber,
            0x05040004 => AbortCode::CrcError,
            0x05040005 => AbortCode::OutOfMemory,
            0x06010000 => AbortCode::UnsupportedAccessToAnObject,
            0x06010001 => AbortCode::AttemptToReadAWriteOnlyObject,
            0x06010002 => AbortCode::AttemptToWriteAReadOnlyObject,
            0x06020000 => AbortCode::ObjectDoesNotExistInTheObjectDictionary,
            0x06040041 => AbortCode::ObjectCannotBeMappedToThePDO,
            0x06040042 => AbortCode::TheNumberAndLengthOfTheObjectsToBeMappedWouldExceedPdoLength,
            0x06040043 => AbortCode::GeneralParameterIncompatibilityReason,
            0x06040047 => AbortCode::GeneralInternalIncompatibilityInTheDevice,
            0x06060000 => AbortCode::AccessFailedDueToAnHardwareError,
            0x06070010 => AbortCode::DataTypeDoesNotMatchLengthOfServiceParameterDoesNotMatch,
            0x06070012 => AbortCode::DataTypeDoesNotMatchLengthOfServiceParameterTooHigh,
            0x06070013 => AbortCode::DataTypeDoesNotMatchLengthOfServiceParameterTooLow,
            0x06090011 => AbortCode::SubIndexDoesNotExist,
            0x06090030 => AbortCode::InvalidValueForParameterDownloadOnly,
            0x06090031 => AbortCode::ValueOfParameterWrittenTooHighDownloadOnly,
            0x06090032 => AbortCode::ValueOfParameterWrittenTooLowDownloadOnly,
            0x06090036 => AbortCode::MaximumValueIsLessThanMinimumValue,
            0x060A0023 => AbortCode::ResourceNotAvailableSdoConnection,
            0x08000000 => AbortCode::GeneralError,
            0x08000020 => AbortCode::DataCannotBeTransferredOrStoredToTheApplication,
0x08000021 => AbortCode::DataCannotBeTransferredOrStoredToTheApplicationBecauseOfLocalControl,
            0x08000022 => AbortCode::DataCannotBeTransferredOrStoredToTheApplicationBecauseOfThePresentDeviceState,
            0x08000023 => AbortCode::ObjectDictionaryDynamicGenerationFailsOrNoObjectDictionaryIsPresent,
            0x08000024 => AbortCode::NoDataAvailable,
            _ => AbortCode::GeneralError,
        }
    }
}
