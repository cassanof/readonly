// readonly is an external CS:GO radar cheat for Linux that only reads the process memory, no writes!

use process_memory::{DataMember, Memory, Pid, TryIntoProcessHandle};
use readonly::{MapChunk, MapInfo, Message, PlayerInfo};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

pub mod signatures {
    pub const DW_CLIENT_STATE: usize = 0xe27a08;
    pub const DW_CLIENT_STATE_STATE: usize = 0x1a0;
    pub const DW_CLIENT_STATE_MAP: usize = 0x324;
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
            .unwrap_or_else(|_| panic!("Failed to read memory at {address:#x}"))
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

// Waits forever until the process is found, then returns the pid and cwd
async fn get_csgo() -> (u32, String) {
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
    let cwd = csgo.cwd().to_str().unwrap().to_string();
    let pid = csgo.pid().to_string().parse::<u32>().unwrap();
    (pid, cwd)
}

// decodes a char buffer into a string. we need to catch the
// null terminator first, then we can decode the string
fn decode_cstring(buf: &[u8]) -> String {
    let mut i = 0;
    while buf[i] != 0 {
        i += 1;
    }
    String::from_utf8_lossy(&buf[..i]).to_string()
}

fn get_map(cwd: &str, name: &str) -> Option<(MapInfo, Vec<MapChunk>)> {
    // let's read the overview file
    let overview_path = format!("{cwd}/csgo/resource/overviews/{name}.txt");
    let overview = std::fs::read_to_string(overview_path).ok()?;
    let mut upper_left_x = None;
    let mut upper_left_y = None;
    let mut scale = None;
    let mut already_rotated = true;

    for line in overview.lines() {
        let mut split = line.split_whitespace();
        let key = split.next().unwrap_or("");
        let value = split.next().unwrap_or("");
        match key {
            "\"pos_x\"" => upper_left_x = value.trim_matches('\"').parse::<f32>().ok(),
            "\"pos_y\"" => upper_left_y = value.trim_matches('\"').parse::<f32>().ok(),
            "\"rotate\"" => already_rotated = value.trim_matches('\"') == "1",
            "\"scale\"" => scale = value.trim_matches('\"').parse::<f32>().ok(),
            _ => continue,
        }
    }

    let upper_left_x = upper_left_x?;
    let upper_left_y = upper_left_y?;
    let scale = scale?;

    // now we need to read the dds file
    let dds_path = format!("{cwd}/csgo/resource/overviews/{name}_radar.dds");
    let img = image::open(dds_path).ok()?;
    // rotate the image if needed
    let img = if already_rotated {
        img
    } else {
        img.rotate180()
    };

    // convert to jpg
    let buf: Vec<u8> = Vec::new();
    let mut writer = std::io::Cursor::new(buf);

    img.write_to(&mut writer, image::ImageOutputFormat::Jpeg(80))
        .ok()?;
    let buf = writer.into_inner();
    let base64 = base64::encode(buf);

    // split the image into 62kb chunks, due to websocket limits
    let mut chunks = Vec::new();
    let mut i = 0;
    let mut n = 0;
    while i < base64.len() {
        let chunk = base64[i..(i + 62 * 1024).min(base64.len())].to_string();
        let chunk = MapChunk { i: n, data: chunk };
        chunks.push(chunk);
        i += 62 * 1024;
        n += 1;
    }

    let map = MapInfo {
        name: name.to_string(),
        chunks: n,
        upper_left_x,
        upper_left_y,
        scale,
    };

    Some((map, chunks))
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

    let (pid, cwd) = get_csgo().await;
    println!("Found process, pid: {pid} --- cwd: {cwd}");

    let handle = (pid as Pid)
        .try_into_process_handle()
        .expect("Failed to open process");
    let cheat = CheatState::new(handle);

    let mut last_map: Option<String> = None;
    let mut i: u64 = 0;

    loop {
        let mut map = None;

        let client_state_state: u32 = cheat.client_state_read(signatures::DW_CLIENT_STATE_STATE);
        if client_state_state != 6 {
            println!("Waiting for client_state_state to be 6...");
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            continue;
        }

        let map_name: [u8; 128] = cheat.client_state_read(signatures::DW_CLIENT_STATE_MAP);
        let map_name = decode_cstring(&map_name);
        println!("map_name: {map_name}");
        if last_map.as_ref() != Some(&map_name) || i % 1000 == 0 {
            map = get_map(&cwd, &map_name);
            if map.is_some() {
                println!("Found map: {map_name}");
            } else {
                println!("Failed to find map: {map_name}");
            }
            last_map = Some(map_name);
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
            // println!("entity_list: {entity_list:#x}");
            if entity_list == 0 {
                continue;
            }

            let entity: u64 = cheat.read(entity_list as usize);
            // println!("entity: {entity:#x}");
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
            let ang_rotation: [f32; 2] = cheat.read(entity as usize + 0x164);

            let mut player = PlayerInfo {
                team,
                is_local: false,
                health,
                position,
                ang_rotation,
            };

            // if the position is the same as the local player, it's probably the local player
            if position == my_position {
                player.is_local = true;
            }
            println!("{player:?}");
            players.push(player);
        }

        if let Some((map, chunks)) = map {
            let json = serde_json::to_vec(&Message::InitMap(map)).unwrap();
            stream.write_all(&json).await.unwrap();
            for chunk in chunks {
                let json = serde_json::to_vec(&Message::MapChunk(chunk)).unwrap();
                stream.write_all(&json).await.unwrap();
            }
        }

        let json = serde_json::to_vec(&Message::Players(players)).unwrap();
        stream.write_all(&json).await.unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        i = i.wrapping_add(1);
    }
}
