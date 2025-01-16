
use crate::sdo::*;
use crate::machine::*;
use crate::dictionary::*;

pub enum ClientRequest {
    InitiateUpload(Index),
    UploadSegment()
};

pub enum ServerResponse {
    InitiateUpload(Index),
    UploadSegment()
};
