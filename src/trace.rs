use libbpf_rs::skel::{OpenSkel, Skel, SkelBuilder};
use libbpf_rs::{Map, MapCore, Object, ProgramMut, RingBufferBuilder};
use std::time::Duration;

const HASH_LENGTH: usize = 32;

#[repr(C)]
pub struct ValidationBlockConnected {
    /// Hash of the connected block
    pub hash: [u8; HASH_LENGTH],
    /// Height of the connected block
    pub height: i32,
    /// Number of transactions in the connected block
    pub transactions: u64,
    /// Number of inputs in the connected block
    pub inputs: i32,
    /// Number of sigops in the connected block
    pub sigops: u64,
    /// Time it took to connect the block in microseconds (µs)
    pub connection_time: u64,
}

impl fmt::Display for ValidationBlockConnected {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "ValidationBlockConnected(hash={}, height={}, transactions={}, inputs={}, sigops={}, time={}µs)",
            "NOT_IMPLEMENTED_HASH", // bitcoin::BlockHash::from_slice(&self.hash).unwrap(),
            self.height, self.transactions, self.inputs, self.sigops, self.connection_time,
        )
    }
}

impl ValidationBlockConnected {
    pub fn from_bytes(x: &[u8]) -> Self {
        unsafe { ptr::read_unaligned(x.as_ptr() as *const Self) }
    }
}

const RINGBUFF_CALLBACK_OK: i32 = 0;
const RINGBUFF_CALLBACK_SYSTEM_TIME_ERROR: i32 = -5;

const NO_EVENTS_ERROR_DURATION: Duration = Duration::from_secs(60 * 3);
const NO_EVENTS_WARN_DURATION: Duration = Duration::from_secs(60 * 1);

struct Tracepoint<'a> {
    pub context: &'a str,
    pub name: &'a str,
    pub function: &'a str,
}

const TRACEPOINTS_VALIDATION: [Tracepoint; 1] = [Tracepoint {
    context: "validation",
    name: "block_connected",
    function: "handle_validation_block_connected",
}];

fn attach_tracepoint(pid: i32, bitcoind_path: String) -> Result<(), RuntimeError> {
    let mut skel_builder = tracing::TracingSkelBuilder::default();
    skel_builder.obj_builder.debug(args.libbpf_debug);

    let mut uninit = MaybeUninit::uninit();
    log::info!("Opening BPF skeleton with debug={}..", args.libbpf_debug);
    let open_skel: tracing::OpenTracingSkel = skel_builder.open(&mut uninit)?;
    log::info!("Loading BPF functions and maps into kernel..");
    let skel: tracing::TracingSkel = open_skel.load()?;
    let obj = skel.object();

    let mut active_tracepoints = vec![];
    let mut ringbuff_builder = RingBufferBuilder::new();

    if active_tracepoints.is_empty() {
        log::error!("No tracepoints enabled.");
        return Ok(());
    }

    // attach tracepoints
    let mut _links = Vec::new();
    for tracepoint in active_tracepoints {
        let prog = find_prog_mut(&obj, tracepoint.function)?;
        _links.push(prog.attach_usdt(
            pid,
            &bitcoind_path,
            tracepoint.context,
            tracepoint.name,
        )?);
        log::info!(
            "hooked the BPF script function {} up to the tracepoint {}:{} of '{}' with PID={}",
            tracepoint.function,
            tracepoint.context,
            tracepoint.name,
            bitcoind_path,
            pid
        );
    }

    let ring_buffers = ringbuff_builder.build()?;
    log::info!(
        "Startup successful. Starting to extract events from '{}'..",
        bitcoind_path
    );
    let mut last_event_timestamp = SystemTime::now();
    let mut has_warned_about_no_events = false;
    loop {
        match ring_buffers.poll_raw(Duration::from_secs(1)) {
            RINGBUFF_CALLBACK_OK => (),
            RINGBUFF_CALLBACK_SYSTEM_TIME_ERROR => log::warn!("SystemTimeError"),
            _other => {
                // values >0 are the number of handled events
                if _other <= 0 {
                    log::warn!("Unhandled ringbuffer callback error: {}", _other)
                } else {
                    last_event_timestamp = SystemTime::now();
                    has_warned_about_no_events = false;
                    log::trace!(
                        "Extracted {} events from ring buffers and published them",
                        _other
                    );
                }
            }
        };
        let duration_since_last_event = SystemTime::now().duration_since(last_event_timestamp)?;
        if duration_since_last_event >= NO_EVENTS_ERROR_DURATION {
            log::error!(
                "No events received in the last {:?}.",
                NO_EVENTS_ERROR_DURATION
            );
            log::warn!("The bitcoind process might be down, has restarted and changed PIDs, or the network might be down.");
            log::warn!("The extractor will exit. Please restart it");
            return Ok(());
        } else if duration_since_last_event >= NO_EVENTS_WARN_DURATION
            && !has_warned_about_no_events
        {
            has_warned_about_no_events = true;
            log::warn!(
                "No events received in the last {:?}. Is bitcoind or the network down?",
                NO_EVENTS_WARN_DURATION
            );
        }
    }
}

fn handle_validation_block_connected(data: &[u8]) -> i32 {
    let connected = ValidationBlockConnected::from_bytes(data);

    // 
}
