// readonly is an external CS:GO radar cheat for Linux that only reads the process memory, no writes!

use process_memory::{DataMember, Memory, Pid, TryIntoProcessHandle};
use readonly::PlayerInfo;
use tokio::{io::AsyncWriteExt, net::TcpStream};

pub mod signatures {
    pub const DW_CLIENT_STATE: usize = 0xe27a08;
    pub const DW_CLIENT_STATE_STATE: usize = 0x1a0;
    pub const LOCAL_PLAYER: usize = 0x234e748;
    pub const ENTITY_LIST: usize = 0x237e560;
    pub const HEALTH: usize = 0x138;
    pub const TEAM: usize = 0x12c;
    pub const POSITION: usize = 0x170;
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
    dotenvy::dotenv().ok();
    let server_name = dotenvy::var("SERVER_NAME").unwrap();
    let tcp_port = dotenvy::var("TCP_PORT").unwrap();
    let api_key = dotenvy::var("API_KEY").unwrap();

    let mut stream = TcpStream::connect(format!("{server_name}:{tcp_port}"))
        .await
        .expect("Failed to connect to TCP server");

    // authenticate
    stream
        .write_all(api_key.as_bytes())
        .await
        .expect("Failed to write to TCP stream");

    let pid = get_csgo_pid().await;
    println!("Found process, pid: {pid}");

    let handle = (pid as Pid)
        .try_into_process_handle()
        .expect("Failed to open process");
    let cheat = CheatState::new(handle);

    loop {
        let client_state_state: u32 = cheat.client_state_read(signatures::DW_CLIENT_STATE_STATE);
        if client_state_state != 6 {
            println!("Waiting for client_state_state to be 6...");
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            continue;
        }

        let max_players: u32 = cheat.client_state_read(0x420);

        println!("client_state_state: {client_state_state:#x} - {client_state_state}");
        println!("max_players: {max_players:#x} - {max_players}");
        let local_player: u32 = cheat.client_read(signatures::LOCAL_PLAYER);
        let my_position: [f32; 3] = cheat.read(local_player as usize + signatures::POSITION);
        println!("my position: {my_position:?}");

        let mut players = Vec::new();

        for i in 0..max_players {
            let entity_list: u64 = cheat.client_read(signatures::ENTITY_LIST + (i * 0x20) as usize);
            if entity_list == 0 {
                break;
            }

            let entity: u32 = cheat.read(entity_list as usize);
            if entity == 0 {
                continue;
            }

            let health: u32 = cheat.read(entity as usize + signatures::HEALTH);
            let team: String = match cheat.read::<u32>(entity as usize + signatures::TEAM) {
                2 => "T",
                3 => "CT",
                _ => "Unknown",
            }
            .to_string();
            let position: [f32; 3] = cheat.read(entity as usize + signatures::POSITION);

            let mut player = PlayerInfo {
                team,
                is_local: false,
                health,
                position,
            };

            // if the position is the same as the local player, it's probably the local player
            if position == my_position {
                player.is_local = true;
            }
            println!("{player:#?}");
            players.push(player);
        }

        let json = serde_json::to_vec(&players).unwrap();
        stream.write_all(&json).await.unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
