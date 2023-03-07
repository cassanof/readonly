// readonly is an external CS:GO radar cheat for Linux that only reads the process memory, no writes!

use process_memory::{DataMember, Memory, Pid, TryIntoProcessHandle};

pub mod signatures {
    pub const DW_CLIENT_STATE: usize = 0xe27a08;
    pub const DW_CLIENT_STATE_STATE: usize = 0x1a0;
}

// Finds the given module name address in the process
struct Permissions {
    read: bool,
    write: bool,
    execute: bool,
}

struct CheatState {
    handle: process_memory::ProcessHandle,
    engine_start: usize,
    client_start: usize,
    client_state: usize,
}

fn memory_read<T: std::marker::Copy>(handle: &process_memory::ProcessHandle, address: usize) -> T {
    unsafe {
        DataMember::new_offset(*handle, vec![address])
            .read()
            .unwrap()
    }
}

impl CheatState {
    fn new(handle: process_memory::ProcessHandle) -> Self {
        let (engine_start, engine_end) = get_module_address(
            &handle,
            "engine_client.so",
            Some(Permissions {
                read: true,
                write: false,
                execute: true,
            }),
        );
        let (client_start, client_end) = get_module_address(
            &handle,
            "client_client.so",
            Some(Permissions {
                read: true,
                write: false,
                execute: true,
            }),
        );
        println!("engine_client.so: {engine_start:#x} - {engine_end:#x}");
        println!("client_client.so: {client_start:#x} - {client_end:#x}");
        let client_state: u32 = memory_read(&handle, engine_start + signatures::DW_CLIENT_STATE);
        let client_state = client_state as usize + 8;
        println!("client_state: {client_state:#x}");

        Self {
            handle,
            engine_start,
            client_start,
            client_state,
        }
    }

    fn read<T: std::marker::Copy>(&self, address: usize) -> T {
        memory_read(&self.handle, address)
    }

    fn engine_read<T: std::marker::Copy>(&self, offset: usize) -> T {
        self.read(self.engine_start + offset)
    }

    fn client_read<T: std::marker::Copy>(&self, offset: usize) -> T {
        self.read(self.client_start + offset)
    }

    fn client_state_read<T: std::marker::Copy>(&self, offset: usize) -> T {
        self.read(self.client_state + offset)
    }
}

// gets the module address range from the given name, and optionally the permissions (if None, it
// returns the first found).
fn get_module_address(
    handle: &process_memory::ProcessHandle,
    module_name: &str,
    perms: Option<Permissions>,
) -> (usize, usize) {
    let pid = handle.0;
    let maps = proc_maps::get_process_maps(pid).expect("Failed to get process maps");
    for map in maps {
        match map.filename() {
            Some(name) if name.ends_with(module_name) => {
                if let Some(ref perms) = perms {
                    if map.is_read() != perms.read
                        || map.is_write() != perms.write
                        || map.is_exec() != perms.execute
                    {
                        continue;
                    }
                }
                let start = map.start();
                return (start, start + map.size());
            }
            _ => continue,
        }
    }
    panic!("Failed to find module address");
}

// Waits forever until the process is found, then returns the pid
async fn get_csgo_pid() -> u32 {
    use sysinfo::{ProcessExt, SystemExt};
    let mut s = sysinfo::System::new_with_specifics(
        sysinfo::RefreshKind::new().with_processes(sysinfo::ProcessRefreshKind::new()),
    );
    let mut csgo = None;
    let mut first = true;
    while csgo.is_none() {
        s.refresh_processes();
        csgo = s.processes_by_name("csgo_linux64").next();
        if csgo.is_none() {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            if first {
                println!("Waiting for csgo_linux64 to start...");
                first = false;
            }
        }
    }
    let csgo = csgo.unwrap();
    // this is kinda dogshit, but whatever...
    csgo.pid().to_string().parse::<u32>().unwrap()
}

#[tokio::main]
async fn main() {
    let pid = get_csgo_pid().await;
    println!("Found process, pid: {pid}");

    let handle = (pid as Pid)
        .try_into_process_handle()
        .expect("Failed to open process");
    let cheat = CheatState::new(handle);
    loop {
        let client_state_state: u32 = cheat.client_state_read(signatures::DW_CLIENT_STATE_STATE);
        println!("client_state_state: {client_state_state:#x} - {client_state_state}");
        let player_crosshair = cheat.client_read::<[u8; 20]>(0x6d0ab84);
        for i in player_crosshair {
            if i == 0 {
                break;
            }
            // ascii
            print!("{i}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
