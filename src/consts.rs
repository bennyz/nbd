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
#[derive(Clone, Copy)]
pub enum NbdOpt {
    ExportName = 1,
    Abort = 2,
    List = 3,
    StartTls = 4,
    Info = 5,
    Go = 6,
    StructuredReply = 7,
    ListMetaContext = 8,
    SetMetaContext = 9,
}

#[repr(u32)]
pub enum NbdReply {
    Ack = 1,
    Server = 2,
    Info = 3,
    MetaContext = 4,

    // Errors
    NbdRepErrUnsup = 1 | NBD_REP_FLAG_ERROR,
}
