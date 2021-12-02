pub const NBD_DEFAULT_PORT: i32 = 10809;
pub const NBD_FLAG_FIXED_NEWSTYLE: u16 = 1 << 0;
pub const NBD_FLAG_NO_ZEROES: u16 = 1 << 1;

pub const NBD_FLAG_C_FIXED_NEWSTYLE: u32 = 1;
pub const NBD_FLAG_C_NO_ZEROES: u32 = 1 << 1;

pub const NBD_INIT_MAGIC: u64 = 0x4e42444d41474943;
pub const NBD_OPTS_MAGIC: u64 = 0x49484156454F5054;
pub const NBD_REP_MAGIC: u64 = 0x3e889045565a9;

// Reply errors
pub const NBD_REP_FLAG_ERROR: u32 = 1 << 31;

// Custom
pub const MIN_BLOCK_SIZE: u32 = 1;
pub const PREFERRED_BLOCK_SIZE: u32 = 4096;
pub const MAX_BLOCK_SIZE: u32 = 1 << 31;

#[repr(u32)]
pub enum NbdCmd {
    Read,
    Write,
    Disc,
    Flush,
    Trim,
    Cache,
    WriteZeroes,
    BlockStatus,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NbdOpt {
    Export = 0,
    ExportName = 1,
    Abort = 2,
    List = 3,
    StartTls = 5,
    Info = 6,
    Go = 7,
    StructuredReply = 8,
    ListMetaContext = 9,
    SetMetaContext = 10,
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NbdReply {
    Ack = 1,
    Server = 2,
    Info = 3,
    MetaContext = 4,

    // Errors
    NbdRepErrUnsup = 1 | NBD_REP_FLAG_ERROR,
}

#[repr(u16)]
#[derive(Debug, Clone, Copy)]
pub enum NbdInfoOpt {
    Export = 0,
    Name = 1,
    Description = 2,
    BlockSize = 3,
    Unknown = 4,
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NbdFlags {
    HasFlags = 0,
    ReadOnly = 1,
    SendFlush = 2,
    SendFua = 3,
    RotationalBit = 4,
    SendTrim = 5,
    SendWriteZeroes = 6,
    SendDf = 7,
    CanMultiConn = 8,
    SendResize = 9,
    SendCache = 10,
    SendFastZero = 11,
}
