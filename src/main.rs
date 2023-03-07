// readonly is an external CS:GO radar cheat for Linux that only reads the process memory, no writes!

// Waits forever until the process is found, then returns the pid
async fn get_csgo_pid() -> u64 {
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
    // this is kinda dogshit, but whatever...
    csgo.unwrap().pid().to_string().parse::<u64>().unwrap()
}

#[tokio::main]
async fn main() {
    println!("Pid: {}", get_csgo_pid().await);

    // find the pid of csgo_linux64
}
