// Server
pub const NBD_DEFAULT_PORT: i32 = 10809;
pub const NBD_FLAG_FIXED_NEWSTYLE: u16 = 1 << 0;
pub const NBD_FLAG_NO_ZEROES: u16 = 1 << 1;

// Client
pub const NBD_FLAG_C_FIXED_NEWSTYLE: u32 = 1;
pub const NBD_FLAG_C_NO_ZEROES: u32 = 1 << 1;

pub const NBD_OPTS_MAGIC: u64 = 0x49484156454F5054;

#[repr(u32)]
pub enum NbdCmd {
    NBD_CMD_READ,
    NBD_CMD_WRITE,
    NBD_CMD_DISC,
    NBD_CMD_FLUSH,
    NBD_CMD_TRIM,
    NBD_CMD_CACHE,
    NBD_CMD_WRITE_ZEROES,
    NBD_CMD_BLOCK_STATUS,
}
