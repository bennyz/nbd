# nbd

An attempt at an NBD server in Rust.

The project is in its very early stages and is a bit of a mess.
However, it is capable of doing things:

## How to use?

```shell
# Clone the project
$ git clone https://github.com/bennyz/nbd.git
$ cd nbd/

# Build it
$ cargo build --release
$ ./target/release/nbd --help
nbd 0.0.1

USAGE:
    nbd [OPTIONS] <FILE> [ARGS]

ARGS:
    <FILE>           The file we want to export
    <NAME>           The name of the export, empty by default [default: ]
    <DESCRIPTION>    The description of the export, empty by default [default: ]

OPTIONS:
    -h, --help       Print help information
        --unix       Whether to use a UNIX socket (additionally) along with the TCP socket by
                     default uses /tmp/nbd.sock, in the future it will be configurable
    -V, --version    Print version information
```

## Examples

Note: These example rely on third-party clients, like `qemu-img`, projects from `nbdkit` (`nbdinfo`) and `nbd-client`

```shell
$ qemu-img create -f raw export-file 1G
Formatting 'export-file', fmt=raw size=1073741824 
$ ./target/release/nbd export-file myexport exporty
Listening on port 10809
```

On a separate shell:

```shell
$ qemu-img info nbd://localhost
image: nbd://localhost:10809
file format: raw
virtual size: 1 GiB (1073741824 bytes)
disk size: unavailable

$ nbdinfo nbd://localhost
protocol: newstyle-fixed without TLS
export="myexport":
        description: exporty
        export-size: 1073741824 (1G)
        content: data
        uri: nbd://localhost:10809/
        is_rotational: false
        is_read_only: false
        can_cache: false
        can_df: false
        can_fast_zero: false
        can_flush: false
        can_fua: false
        can_multi_conn: true
        can_trim: false
        can_zero: false
        block_size_minimum: 1
        block_size_preferred: 4096
        block_size_maximum: 33554432
```

We can even create a usable file system!

```shell
$ sudo modprobe nbd
$ sudo nbd-client localhost 10809 /dev/nbd0
Warning: the oldstyle protocol is no longer supported.
This method now uses the newstyle protocol with a default export
Negotiation: ..size = 1024MB
Connected /dev/nbd0

# Now our export is available via /dev/nbd0
# Let's create an XFS filesystem on it!
$ sudo mkfs.xfs /dev/nbd0
meta-data=/dev/nbd0              isize=512    agcount=4, agsize=65536 blks
         =                       sectsz=512   attr=2, projid32bit=1
         =                       crc=1        finobt=1, sparse=1, rmapbt=0
         =                       reflink=1    bigtime=0 inobtcount=0
data     =                       bsize=4096   blocks=262144, imaxpct=25
         =                       sunit=0      swidth=0 blks
naming   =version 2              bsize=4096   ascii-ci=0, ftype=1
log      =internal log           bsize=4096   blocks=2560, version=2
         =                       sectsz=512   sunit=0 blks, lazy-count=1
realtime =none                   extsz=4096   blocks=0, rtextents=0

# We can then mount it and use it!
$ sudo mount /dev/nbd0 mnt
$ touch mnt/first_file

# Close things up
$ sudo umount /dev/nbd0
$ sudo nbd-client -d /dev/nbd0

# We can then reconnect
$ sudo nbd-client localhost 10809 /dev/nbd0
Warning: the oldstyle protocol is no longer supported.
This method now uses the newstyle protocol with a default export
Negotiation: ..size = 1024MB
Connected /dev/nbd0

$ sudo mount /dev/nbd0 mnt
# The file is there!
$ ls mnt
first_file
```
